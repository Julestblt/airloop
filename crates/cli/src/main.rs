//! `airloop-cli` — headless sender for protocol testing and diagnostics.
//!
//! ```text
//! airloop-cli scan [--seconds N]
//! airloop-cli tone   <speaker> [<speaker>...] [--volume P] [--seconds N] [--freq HZ]
//! airloop-cli capture <speaker> [<speaker>...] [--volume P] [--seconds N] [--pid PID]   (Windows)
//! airloop-cli sources                                                              (Windows)
//! ```
//!
//! `<speaker>` matches a speaker id or a case-insensitive name prefix. Volume
//! is 0–100 on the same scale as the app and defaults to a quiet 25.

use airloop_engine::{AudioSource, Engine, EngineState, ToneSource};
use anyhow::{bail, Context, Result};
use std::time::{Duration, Instant};

struct Args {
    command: String,
    speakers: Vec<String>,
    volume: u8,
    seconds: u64,
    freq: f32,
    pid: Option<u32>,
}

fn parse() -> Result<Args> {
    let mut it = std::env::args().skip(1);
    let command = it.next().unwrap_or_else(|| "help".into());
    let mut a = Args {
        command,
        speakers: Vec::new(),
        volume: airloop_engine::volume::DEFAULT_PERCENT,
        seconds: 10,
        freq: 440.0,
        pid: None,
    };
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().with_context(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--volume" => a.volume = value("--volume")?.parse()?,
            "--seconds" => a.seconds = value("--seconds")?.parse()?,
            "--freq" => a.freq = value("--freq")?.parse()?,
            "--pid" => a.pid = Some(value("--pid")?.parse()?),
            s if s.starts_with("--") => bail!("unknown option {s}"),
            s => a.speakers.push(s.to_string()),
        }
    }
    Ok(a)
}

fn resolve(engine: &Engine, wanted: &[String]) -> Result<Vec<String>> {
    let speakers = engine.current_speakers();
    wanted
        .iter()
        .map(|w| {
            let lw = w.to_lowercase();
            speakers
                .iter()
                .find(|s| s.id == *w || s.name.to_lowercase() == lw)
                .or_else(|| {
                    speakers
                        .iter()
                        .find(|s| s.name.to_lowercase().starts_with(&lw))
                })
                .map(|s| s.id.clone())
                .with_context(|| format!("no speaker matches {w:?}"))
        })
        .collect()
}

async fn stream(args: &Args, source: Box<dyn AudioSource>) -> Result<()> {
    if args.speakers.is_empty() {
        bail!("name at least one speaker");
    }
    let engine = Engine::start(args.volume, false).await?;
    // Give mDNS time to see every member of a stereo pair, not just the
    // first advertisement.
    tokio::time::sleep(Duration::from_secs(4)).await;
    let ids = resolve(&engine, &args.speakers)?;
    println!(
        "streaming {} to {ids:?} at volume {}",
        source.describe(),
        args.volume
    );

    let t0 = Instant::now();
    engine.connect(ids, source).await;
    let outcome = watch_session(args, &engine, t0).await;
    engine.disconnect().await;
    let mut status = engine.status();
    // Let teardown finish.
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        status.wait_for(|s| s.state == EngineState::Idle),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    outcome
}

async fn watch_session(args: &Args, engine: &Engine, t0: Instant) -> Result<()> {
    let mut status = engine.status();
    let mut streaming_at = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(args.seconds + 30);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = tokio::time::sleep_until(deadline) => bail!("timed out"),
            changed = status.changed() => {
                if changed.is_err() { break; }
                let s = status.borrow_and_update().clone();
                match &s.state {
                    EngineState::Streaming if streaming_at.is_none() => {
                        println!("streaming after {:.1}s", t0.elapsed().as_secs_f32());
                        streaming_at = Some(tokio::time::Instant::now());
                    }
                    EngineState::Error => bail!("{}", s.error.map(|e| e.detail).unwrap_or_default()),
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(1)), if streaming_at.is_some() => {
                let s = engine.status().borrow().clone();
                println!("level {:.2} diag {}", engine.take_level(), serde_json::to_string(&s.diagnostics)?);
                if streaming_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(args.seconds)) {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "info,airplay_audio=warn,airplay_client=warn,airplay_timing=warn".into()
            }),
        )
        .with_writer(std::io::stderr)
        .init();
    let args = parse()?;
    // Hard watchdog: whatever happens, never keep streaming past the requested
    // duration plus generous connect/teardown time.
    let limit = Duration::from_secs(args.seconds + 60);
    std::thread::spawn(move || {
        std::thread::sleep(limit);
        eprintln!("watchdog: exceeded {limit:?}, exiting");
        std::process::exit(3);
    });
    let result = run(&args).await;
    if let Err(e) = &result {
        eprintln!("error: {e:#}");
    }
    // Exit explicitly: dropping the runtime would wait for blocking tasks.
    std::process::exit(i32::from(result.is_err()));
}

async fn run(args: &Args) -> Result<()> {
    match args.command.as_str() {
        "scan" => {
            let engine = Engine::start(0, false).await?;
            tokio::time::sleep(Duration::from_secs(args.seconds.min(30))).await;
            for s in engine.current_speakers() {
                println!("{}", serde_json::to_string(&s)?);
            }
        }
        "tone" => {
            // -18 dBFS: a clearly audible but gentle test signal.
            let tone = ToneSource::new(args.freq, 0.125, None);
            stream(args, Box::new(tone)).await?;
        }
        "capture" => {
            let source = airloop_capture::open_source(match args.pid {
                Some(pid) => airloop_capture::CaptureTarget::Process { pid },
                None => airloop_capture::CaptureTarget::System,
            })?;
            stream(args, source).await?;
        }
        "sources" => {
            for app in airloop_capture::list_audio_apps()? {
                println!("{}", serde_json::to_string(&app)?);
            }
        }
        _ => {
            eprintln!("usage: airloop-cli scan | tone <speaker>... | capture <speaker>... [--pid PID] | sources");
            eprintln!("options: --volume 0-100 (default 25)  --seconds N  --freq HZ");
        }
    }
    Ok(())
}
