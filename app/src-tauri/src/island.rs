//! Notch-style reveal for the island window.
//!
//! The window has a fixed size and stays hidden until the mouse touches the
//! top centre of a screen (the "hot zone"). A watcher thread polls the global
//! cursor position:
//!
//! - hidden + cursor in the hot zone → place the window on that monitor, show
//!   it without taking focus, emit `island:reveal`;
//! - shown → the transparent parts of the window let clicks through (only the
//!   island itself, whose bounds the UI reports, receives the mouse);
//! - shown + cursor away from the island for a moment → emit `island:dismiss`;
//!   the UI plays its exit animation and calls `island_hidden`, which hides
//!   the window.
//!
//! The UI can also reveal the island for a few seconds (status changes) and
//! pin it while the user drags a slider.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, PhysicalPosition, WebviewWindow};
use tracing::{debug, warn};

/// Fixed logical size of the window. The island (pill or panel) is drawn at
/// its top centre; the rest is transparent and click-through.
pub const WINDOW_W: f64 = 600.0;
pub const WINDOW_H: f64 = 580.0;
/// Half-width of the hot zone, in logical px either side of the centre.
const HOT_HALF_WIDTH: f64 = 170.0;
/// How close to the top edge (physical px) counts as "touching" it.
const HOT_EDGE: f64 = 2.0;
/// Slack around the island before the cursor counts as gone.
const INSIDE_MARGIN: f64 = 14.0;
/// How long the cursor may be away before the island folds away.
const LEAVE_GRACE: Duration = Duration::from_millis(450);
const TICK: Duration = Duration::from_millis(30);
/// The cursor must rest against the edge this long before the island
/// appears, so flicking up to a maximized window's tabs doesn't trigger it.
const HOT_DWELL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, Default)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Default)]
pub struct IslandControl {
    /// Island bounds in logical px, relative to the window.
    rect: Mutex<Option<Rect>>,
    pinned: AtomicBool,
    reveal_until: Mutex<Option<Instant>>,
    shown: AtomicBool,
    hiding: AtomicBool,
    /// When the UI was asked to fold away; used as a fallback if it never
    /// reports back.
    dismissed_at: Mutex<Option<Instant>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl IslandControl {
    pub fn set_rect(&self, x: f64, y: f64, w: f64, h: f64) {
        *lock(&self.rect) = Some(Rect { x, y, w, h });
    }

    pub fn set_pinned(&self, pinned: bool) {
        self.pinned.store(pinned, Ordering::Release);
    }

    /// Keeps the island shown for at least `ms`, revealing it if needed.
    pub fn reveal_for(&self, ms: u64) {
        let until = Instant::now() + Duration::from_millis(ms);
        let mut g = lock(&self.reveal_until);
        if g.is_none_or(|t| t < until) {
            *g = Some(until);
        }
    }

    /// Called by the UI once its exit animation has finished.
    pub fn hidden(&self, window: &WebviewWindow) {
        if self.hiding.swap(false, Ordering::AcqRel) {
            let _ = window.hide();
            self.shown.store(false, Ordering::Release);
        }
    }
}

/// Puts the window at the top centre of the monitor containing `point`.
fn place(window: &WebviewWindow, point: PhysicalPosition<f64>) {
    let monitor = window
        .monitor_from_point(point.x, point.y)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let x = m.position().x as f64 / scale + (m.size().width as f64 / scale - WINDOW_W) / 2.0;
        let y = m.position().y as f64 / scale;
        let _ = window.set_position(LogicalPosition::new(x.round(), y.round()));
    }
}

fn in_hot_zone(window: &WebviewWindow, c: PhysicalPosition<f64>) -> bool {
    let Ok(Some(m)) = window.monitor_from_point(c.x, c.y) else {
        return false;
    };
    let scale = m.scale_factor();
    let top = m.position().y as f64;
    let centre = m.position().x as f64 + m.size().width as f64 / 2.0;
    c.y <= top + HOT_EDGE && (c.x - centre).abs() <= HOT_HALF_WIDTH * scale
}

/// Prepares the window and starts the watcher thread.
pub fn start(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        warn!("island window missing");
        return;
    };
    // Never steal focus from the app the user is typing in.
    if let Err(e) = window.set_focusable(false) {
        debug!(error = %e, "set_focusable unsupported");
    }
    let _ = window.set_size(tauri::LogicalSize::new(WINDOW_W, WINDOW_H));
    let _ = window.hide();

    let app = app.clone();
    std::thread::Builder::new()
        .name("island-watcher".into())
        .spawn(move || watch(&app, &window))
        .map_err(|e| warn!(error = %e, "could not start island watcher"))
        .ok();
}

fn watch(app: &AppHandle, window: &WebviewWindow) {
    let ctl = app.state::<IslandControl>();
    let mut last_inside = Instant::now();
    let mut ignoring = false;
    let mut hot_since: Option<Instant> = None;
    loop {
        std::thread::sleep(TICK);
        let Ok(cursor) = app.cursor_position() else {
            continue;
        };
        let now = Instant::now();
        let revealing = lock(&ctl.reveal_until).is_some_and(|t| t > now);

        if !ctl.shown.load(Ordering::Acquire) {
            let hot = in_hot_zone(window, cursor);
            hot_since = if hot { hot_since.or(Some(now)) } else { None };
            let dwelled = hot_since.is_some_and(|t| now.duration_since(t) >= HOT_DWELL);
            if revealing || dwelled {
                hot_since = None;
                place(window, cursor);
                let _ = window.set_ignore_cursor_events(false);
                ignoring = false;
                let _ = window.show();
                ctl.shown.store(true, Ordering::Release);
                ctl.hiding.store(false, Ordering::Release);
                last_inside = now;
                let _ = app.emit("island:reveal", ());
            }
            continue;
        }

        // Shown: is the cursor over the island?
        let inside = match (
            window.outer_position(),
            window.scale_factor(),
            *lock(&ctl.rect),
        ) {
            (Ok(pos), Ok(scale), Some(r)) => {
                let x = (cursor.x - pos.x as f64) / scale;
                let y = (cursor.y - pos.y as f64) / scale;
                x >= r.x - INSIDE_MARGIN
                    && x <= r.x + r.w + INSIDE_MARGIN
                    && y >= r.y - INSIDE_MARGIN
                    && y <= r.y + r.h + INSIDE_MARGIN
            }
            _ => false,
        } || in_hot_zone(window, cursor);

        // Transparent areas let clicks through to whatever is underneath.
        if inside == ignoring {
            ignoring = !inside;
            let _ = window.set_ignore_cursor_events(ignoring);
        }

        if inside || revealing || ctl.pinned.load(Ordering::Acquire) {
            last_inside = now;
            if ctl.hiding.swap(false, Ordering::AcqRel) {
                // Came back while it was folding away.
                let _ = app.emit("island:reveal", ());
            }
        } else if now.duration_since(last_inside) >= LEAVE_GRACE {
            if !ctl.hiding.swap(true, Ordering::AcqRel) {
                *lock(&ctl.dismissed_at) = Some(now);
                let _ = app.emit("island:dismiss", ());
            } else if lock(&ctl.dismissed_at)
                .is_some_and(|t| now.duration_since(t) > Duration::from_millis(1500))
            {
                // The UI did not answer; hide anyway.
                ctl.hidden(window);
            }
        }
    }
}
