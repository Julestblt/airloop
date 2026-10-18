// The app window is a frameless, transparent, always-on-top strip pinned to
// the top centre of the screen. It is sized to the island it contains, plus
// some transparent margin so the island's drop shadow is not clipped.
// Outside Tauri (browser dev server, tests) every function is a no-op.

export const MARGIN = { top: 8, side: 28, bottom: 34 } as const;

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Resizes the window around an island of `width` × `height` CSS px and keeps
 * it centred at the top of the monitor it is on. */
export async function fitWindow(width: number, height: number): Promise<void> {
  if (!inTauri) return;
  const { getCurrentWindow, currentMonitor } = await import("@tauri-apps/api/window");
  const { LogicalPosition, LogicalSize } = await import("@tauri-apps/api/dpi");
  const win = getCurrentWindow();
  const w = Math.ceil(width + MARGIN.side * 2);
  const h = Math.ceil(height + MARGIN.top + MARGIN.bottom);
  await win.setSize(new LogicalSize(w, h));
  const monitor = await currentMonitor();
  if (monitor) {
    const scale = monitor.scaleFactor;
    const x = monitor.position.x / scale + (monitor.size.width / scale - w) / 2;
    const y = monitor.position.y / scale;
    await win.setPosition(new LogicalPosition(Math.round(x), Math.round(y)));
  }
}

/** Shows and focuses the window (it starts hidden until first sized). */
export async function showWindow(): Promise<void> {
  if (!inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const win = getCurrentWindow();
  await win.show();
  await win.setFocus();
}

/** Calls `cb` whenever the window loses focus. Returns an unsubscribe. */
export async function onWindowBlur(cb: () => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (!focused) cb();
  });
}

/** Calls `cb` when the tray asks the island to open. */
export async function onExpandRequest(cb: () => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen("island:expand", () => cb());
}
