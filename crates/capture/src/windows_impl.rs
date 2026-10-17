//! WASAPI capture backend.

use crate::{pretty_app_name, AudioApp, CaptureTarget};
use airloop_engine::{AudioSource, FrameSink, SourceError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use wasapi::{
    initialize_mta, AudioCaptureClient, AudioClient, DeviceEnumerator, Direction, SampleType,
    SessionState, StreamMode, WaveFormat,
};
use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

const RATE: u32 = airloop_engine::SAMPLE_RATE;
/// How often to look for a new default device / a dead process.
const HOUSEKEEPING: Duration = Duration::from_secs(1);
/// Consecutive failed reopen attempts before giving up.
const MAX_REOPEN: u32 = 10;

fn err(e: impl std::fmt::Display) -> SourceError {
    SourceError::Backend(e.to_string())
}

/// Process loopback capture needs Windows 10 build 20348 or Windows 11.
pub fn supports_process_loopback() -> bool {
    windows_build() >= 20_348
}

fn windows_build() -> u32 {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a properly sized, initialised OSVERSIONINFOW.
    let status = unsafe { RtlGetVersion(&mut info) };
    if status.is_ok() {
        info.dwBuildNumber
    } else {
        0
    }
}

pub struct WasapiSource {
    target: CaptureTarget,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl WasapiSource {
    pub fn new(target: CaptureTarget) -> Self {
        Self {
            target,
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }
}

impl AudioSource for WasapiSource {
    fn describe(&self) -> String {
        match self.target {
            CaptureTarget::System => "System audio".into(),
            CaptureTarget::Process { pid } => {
                let name = process_exe(pid)
                    .map(|e| pretty_app_name(&e))
                    .unwrap_or_else(|| format!("process {pid}"));
                format!("{name} (app audio)")
            }
        }
    }

    fn start(&mut self, sink: FrameSink) -> Result<(), SourceError> {
        if let CaptureTarget::Process { pid } = self.target {
            if !supports_process_loopback() {
                return Err(SourceError::Backend(
                    "capturing a single app needs Windows 11 or Windows 10 build 20348+".into(),
                ));
            }
            if !process_alive(pid) {
                return Err(SourceError::ProcessGone(pid));
            }
        }
        self.running.store(true, Ordering::Release);
        let running = Arc::clone(&self.running);
        let target = self.target;
        let thread = std::thread::Builder::new()
            .name("wasapi-capture".into())
            .spawn(move || capture_thread(target, &sink, &running))
            .map_err(err)?;
        self.thread = Some(thread);
        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for WasapiSource {
    fn drop(&mut self) {
        self.stop();
    }
}

enum Outcome {
    /// Asked to stop, or the captured process exited.
    Stop,
    /// The device changed or was invalidated; open again.
    Reopen,
}

fn capture_thread(target: CaptureTarget, sink: &FrameSink, running: &AtomicBool) {
    // S_FALSE (already initialised) is fine; RPC_E_CHANGED_MODE would mean
    // someone made this thread STA, which cannot happen on a fresh thread.
    let _ = initialize_mta();
    boost_thread_priority();
    let mut failures = 0;
    while running.load(Ordering::Acquire) {
        match run_stream(target, sink, running) {
            Ok(Outcome::Stop) => break,
            Ok(Outcome::Reopen) => {
                failures = 0;
                info!("reopening capture stream");
            }
            Err(e) => {
                failures += 1;
                warn!(error = %e, failures, "capture stream failed");
                if failures >= MAX_REOPEN {
                    sink.report_error(e.to_string());
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    debug!("capture thread exiting");
}

/// Asks MMCSS to schedule this thread as audio ("Pro Audio") so capture does
/// not starve when the machine is busy.
fn boost_thread_priority() {
    use windows::core::w;
    use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
    let mut index = 0u32;
    // SAFETY: valid null-terminated task name and out-pointer.
    if let Err(e) = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) } {
        debug!(error = %e, "MMCSS registration failed");
    }
}

struct OpenStream {
    client: AudioClient,
    capture: AudioCaptureClient,
    format: WaveFormat,
    device_id: Option<String>,
}

fn desired_format() -> WaveFormat {
    WaveFormat::new(32, 32, &SampleType::Float, RATE as usize, 2, None)
}

fn open(target: CaptureTarget) -> Result<OpenStream, SourceError> {
    match target {
        CaptureTarget::System => {
            let enumerator = DeviceEnumerator::new().map_err(err)?;
            let device = enumerator
                .get_default_device(&Direction::Render)
                .map_err(err)?;
            let device_id = device.get_id().ok();
            let name = device.get_friendlyname().unwrap_or_default();
            // Preferred: let Windows convert to 44.1 kHz stereo float for us.
            let mut client = device.get_iaudioclient().map_err(err)?;
            let mode = StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 2_000_000,
            };
            let format =
                match client.initialize_client(&desired_format(), &Direction::Capture, &mode) {
                    Ok(()) => desired_format(),
                    Err(e) => {
                        // Fallback: the engine's mix format, resampled by us.
                        warn!(error = %e, "auto-convert loopback refused, using the mix format");
                        client = device.get_iaudioclient().map_err(err)?;
                        let mix = client.get_mixformat().map_err(err)?;
                        let mode = StreamMode::EventsShared {
                            autoconvert: false,
                            buffer_duration_hns: 2_000_000,
                        };
                        client
                            .initialize_client(&mix, &Direction::Capture, &mode)
                            .map_err(err)?;
                        mix
                    }
                };
            info!(device = %name, rate = format.get_samplespersec(), channels = format.get_nchannels(), "system loopback opened");
            let capture = client.get_audiocaptureclient().map_err(err)?;
            Ok(OpenStream {
                client,
                capture,
                format,
                device_id,
            })
        }
        CaptureTarget::Process { pid } => {
            let mut client =
                AudioClient::new_application_loopback_client(pid, true).map_err(err)?;
            let mode = StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 0,
            };
            client
                .initialize_client(&desired_format(), &Direction::Capture, &mode)
                .map_err(err)?;
            info!(pid, "process loopback opened");
            let capture = client.get_audiocaptureclient().map_err(err)?;
            Ok(OpenStream {
                client,
                capture,
                format: desired_format(),
                device_id: None,
            })
        }
    }
}

fn default_render_id() -> Option<String> {
    DeviceEnumerator::new()
        .ok()?
        .get_default_device(&Direction::Render)
        .ok()?
        .get_id()
        .ok()
}

fn run_stream(
    target: CaptureTarget,
    sink: &FrameSink,
    running: &AtomicBool,
) -> Result<Outcome, SourceError> {
    let stream = open(target)?;
    let event = stream.client.set_get_eventhandle().map_err(err)?;
    let mut converter = Converter::new(&stream.format)?;
    let block = stream.format.get_blockalign() as usize;
    let mut raw = vec![0u8; block * 4096];
    stream.client.start_stream().map_err(err)?;

    let mut last_check = Instant::now();
    let outcome = loop {
        if !running.load(Ordering::Acquire) {
            break Outcome::Stop;
        }
        // Loopback only signals while something is playing; a timeout just
        // means silence, which the engine's buffer turns into clean silence.
        let _ = event.wait_for_event(100);

        loop {
            let frames = match stream.capture.get_next_packet_size().map_err(err)? {
                Some(0) | None => break,
                Some(n) => n as usize,
            };
            if raw.len() < frames * block {
                raw.resize(frames * block, 0);
            }
            let (got, info) = stream.capture.read_from_device(&mut raw).map_err(err)?;
            let bytes = &mut raw[..got as usize * block];
            if info.flags.silent {
                bytes.fill(0);
            }
            converter.push(bytes, sink);
        }

        if last_check.elapsed() >= HOUSEKEEPING {
            last_check = Instant::now();
            match target {
                CaptureTarget::System => {
                    let now = default_render_id();
                    if now.is_some() && now != stream.device_id {
                        info!("default output device changed");
                        break Outcome::Reopen;
                    }
                }
                CaptureTarget::Process { pid } => {
                    if !process_alive(pid) {
                        sink.report_error("The captured app was closed.");
                        break Outcome::Stop;
                    }
                }
            }
        }
    };
    let _ = stream.client.stop_stream();
    Ok(outcome)
}

/// Converts captured bytes to interleaved f32 at 44.1 kHz.
struct Converter {
    sample: SampleKind,
    channels: usize,
    resampler: Option<Resampler>,
    scratch: Vec<f32>,
}

#[derive(Clone, Copy)]
enum SampleKind {
    F32,
    I16,
    I24,
    I32,
}

impl Converter {
    fn new(format: &WaveFormat) -> Result<Self, SourceError> {
        let kind = match (
            format.get_subformat().map_err(err)?,
            format.get_bitspersample(),
        ) {
            (SampleType::Float, 32) => SampleKind::F32,
            (SampleType::Int, 16) => SampleKind::I16,
            (SampleType::Int, 24) => SampleKind::I24,
            (SampleType::Int, 32) => SampleKind::I32,
            (t, b) => {
                return Err(SourceError::Backend(format!(
                    "unsupported capture format {t:?} {b}-bit"
                )))
            }
        };
        let channels = usize::from(format.get_nchannels());
        let rate = format.get_samplespersec();
        let resampler = (rate != RATE).then(|| Resampler::new(rate, RATE, channels));
        Ok(Self {
            sample: kind,
            channels,
            resampler,
            scratch: Vec::new(),
        })
    }

    fn push(&mut self, bytes: &[u8], sink: &FrameSink) {
        self.scratch.clear();
        match self.sample {
            SampleKind::F32 => self.scratch.extend(
                bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            ),
            SampleKind::I16 => self.scratch.extend(
                bytes
                    .chunks_exact(2)
                    .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0),
            ),
            SampleKind::I24 => self.scratch.extend(
                bytes
                    .chunks_exact(3)
                    .map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0),
            ),
            SampleKind::I32 => self.scratch.extend(
                bytes
                    .chunks_exact(4)
                    .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0),
            ),
        }
        let channels = self.channels as u16;
        match &mut self.resampler {
            None => sink.push(&self.scratch, channels),
            Some(r) => {
                let out = r.process(&self.scratch);
                if !out.is_empty() {
                    sink.push(&out, channels);
                }
            }
        }
    }
}

/// Band-limited windowed-sinc resampler for the fallback path (mix format ≠
/// 44.1 kHz). Fixed ratio; drift is handled downstream by the engine.
struct Resampler {
    inner: rubato::FftFixedIn<f32>,
    channels: usize,
    pending: Vec<Vec<f32>>,
}

impl Resampler {
    fn new(from: u32, to: u32, channels: usize) -> Self {
        let inner = rubato::FftFixedIn::<f32>::new(from as usize, to as usize, 480, 2, channels)
            .unwrap_or_else(|e| panic!("valid resampler parameters ({from}->{to}): {e}"));
        Self {
            inner,
            channels,
            pending: vec![Vec::new(); channels],
        }
    }

    fn process(&mut self, interleaved: &[f32]) -> Vec<f32> {
        use rubato::Resampler as _;
        for frame in interleaved.chunks_exact(self.channels) {
            for (c, s) in frame.iter().enumerate() {
                self.pending[c].push(*s);
            }
        }
        let mut out = Vec::new();
        loop {
            let need = self.inner.input_frames_next();
            if self.pending[0].len() < need {
                break;
            }
            let chunk: Vec<Vec<f32>> = self
                .pending
                .iter_mut()
                .map(|ch| ch.drain(..need).collect())
                .collect();
            match self.inner.process(&chunk, None) {
                Ok(res) => {
                    let n = res[0].len();
                    for i in 0..n {
                        for ch in &res {
                            out.push(ch[i]);
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "resampler error");
                    break;
                }
            }
        }
        out
    }
}

fn process_alive(pid: u32) -> bool {
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code).is_ok();
        let _ = CloseHandle(h);
        ok && code == STILL_ACTIVE.0 as u32
    }
}

struct ProcInfo {
    parent: u32,
    exe: String,
}

fn process_table() -> HashMap<u32, ProcInfo> {
    let mut map = HashMap::new();
    // SAFETY: standard ToolHelp iteration; the snapshot handle is closed.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return map;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let len = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
            map.insert(
                entry.th32ProcessID,
                ProcInfo {
                    parent: entry.th32ParentProcessID,
                    exe,
                },
            );
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    map
}

fn process_exe(pid: u32) -> Option<String> {
    process_table().remove(&pid).map(|p| p.exe)
}

/// Walks up the parent chain while the parent runs the same executable, so
/// that e.g. Chrome's audio-service child maps to the main browser process.
fn app_root(pid: u32, table: &HashMap<u32, ProcInfo>) -> u32 {
    let mut current = pid;
    for _ in 0..16 {
        let Some(me) = table.get(&current) else { break };
        match table.get(&me.parent) {
            Some(parent) if parent.exe.eq_ignore_ascii_case(&me.exe) && me.parent != current => {
                current = me.parent
            }
            _ => break,
        }
    }
    current
}

pub fn list_audio_apps() -> Result<Vec<AudioApp>, SourceError> {
    let _ = initialize_mta();
    let enumerator = DeviceEnumerator::new().map_err(err)?;
    let devices = enumerator
        .get_device_collection(&Direction::Render)
        .map_err(err)?;
    let table = process_table();
    let me = std::process::id();
    let mut apps: HashMap<u32, AudioApp> = HashMap::new();

    for device in &devices {
        let Ok(device) = device else { continue };
        let Ok(manager) = device.get_iaudiosessionmanager() else {
            continue;
        };
        let Ok(sessions) = manager.get_audiosessionenumerator() else {
            continue;
        };
        let count = sessions.get_count().unwrap_or(0);
        for i in 0..count {
            let Ok(control) = sessions.get_session(i) else {
                continue;
            };
            let state = control.get_state().unwrap_or(SessionState::Expired);
            if state == SessionState::Expired {
                continue;
            }
            let Ok(pid) = control.get_process_id() else {
                continue;
            };
            if pid == 0 || pid == me {
                continue; // system sounds / ourselves
            }
            let root = app_root(pid, &table);
            let Some(exe) = table.get(&root).map(|p| p.exe.clone()) else {
                continue;
            };
            let active = state == SessionState::Active;
            apps.entry(root)
                .and_modify(|a| a.active |= active)
                .or_insert_with(|| AudioApp {
                    pid: root,
                    name: pretty_app_name(&exe),
                    exe,
                    active,
                });
        }
    }

    let mut list: Vec<AudioApp> = apps.into_values().collect();
    list.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(list)
}
