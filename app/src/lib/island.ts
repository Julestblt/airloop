// Bridge to the native island controller (app/src-tauri/src/island.rs).
//
// The window has a fixed size and is hidden until the mouse touches the top
// centre of the screen; the native side shows it and emits `island:reveal`,
// later `island:dismiss` when the mouse has left. The UI animates in and out
// and reports the island's bounds so the rest of the window lets clicks
// through. Outside Tauri (browser dev server, tests) these are no-ops and the
// island is simply always visible.

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invoke(cmd: string, args?: Record<string, unknown>): Promise<void> {
  if (!inTauri) return;
  const core = await import("@tauri-apps/api/core");
  await core.invoke(cmd, args);
}

/** Island bounds in CSS px relative to the window. */
export function reportRect(r: DOMRect): Promise<void> {
  return invoke("island_rect", { x: r.x, y: r.y, w: r.width, h: r.height });
}

/** Keeps the island up while the user drags something (e.g. the volume). */
export function pin(pinned: boolean): Promise<void> {
  return invoke("island_pin", { pinned });
}

/** Shows the island for `ms` even if the mouse is elsewhere. */
export function reveal(ms: number): Promise<void> {
  return invoke("island_reveal", { ms });
}

/** Tells the native side the exit animation finished; it hides the window. */
export function hidden(): Promise<void> {
  return invoke("island_hidden");
}

async function on(event: string, cb: () => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen(event, () => cb());
}

export const onReveal = (cb: () => void) => on("island:reveal", cb);
export const onDismiss = (cb: () => void) => on("island:dismiss", cb);
export const onExpandRequest = (cb: () => void) => on("island:expand", cb);
