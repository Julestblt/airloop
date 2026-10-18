// Thin wrapper over Tauri commands and events. Outside Tauri (plain `vite`
// dev server, unit tests) it falls back to an in-memory mock so the UI can be
// developed and tested without the native side.

import type { AppInfo, AudioApp, EngineStatus, Settings, SourceChoice, Speaker } from "./types";

type Unlisten = () => void;

export interface Api {
  appInfo(): Promise<AppInfo>;
  speakers(): Promise<Speaker[]>;
  status(): Promise<EngineStatus>;
  settings(): Promise<Settings>;
  saveSettings(s: Settings): Promise<void>;
  listApps(): Promise<AudioApp[]>;
  connect(speakerIds: string[], source: SourceChoice): Promise<void>;
  disconnect(): Promise<void>;
  setVolume(volume: number): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  setAutoReconnect(enabled: boolean): Promise<void>;
  level(): Promise<number>;
  openLogDir(): Promise<void>;
  fixFirewall(): Promise<void>;
  quit(): Promise<void>;
  onSpeakers(cb: (s: Speaker[]) => void): Promise<Unlisten>;
  onStatus(cb: (s: EngineStatus) => void): Promise<Unlisten>;
}

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  return {
    appInfo: () => invoke("app_info"),
    speakers: () => invoke("get_speakers"),
    status: () => invoke("get_status"),
    settings: () => invoke("get_settings"),
    saveSettings: (settings) => invoke("save_settings", { settings }),
    listApps: () => invoke("list_apps"),
    connect: (speakerIds, source) => invoke("connect", { speakerIds, source }),
    disconnect: () => invoke("disconnect"),
    setVolume: (volume) => invoke("set_volume", { volume }),
    pause: () => invoke("pause"),
    resume: () => invoke("resume"),
    setAutoReconnect: (enabled) => invoke("set_auto_reconnect", { enabled }),
    level: () => invoke("get_level"),
    openLogDir: () => invoke("open_log_dir"),
    fixFirewall: () => invoke("fix_firewall"),
    quit: () => invoke("quit_app"),
    onSpeakers: (cb) => listen<Speaker[]>("speakers", (e) => cb(e.payload)),
    onStatus: (cb) => listen<EngineStatus>("status", (e) => cb(e.payload)),
  };
}

export const idleStatus: EngineStatus = {
  state: { kind: "idle" },
  speakerIds: [],
  volume: 25,
  source: null,
  error: null,
  autoReconnect: true,
  diagnostics: { bufferMs: 0, driftPpm: 0, bufferUnderruns: 0, droppedChunks: 0, speakers: [] },
};

export function mockApi(): Api {
  const speakers: Speaker[] = [
    {
      id: "pair-demo",
      name: "Living Room",
      model: "AudioAccessory5,1",
      modelName: "HomePod mini stereo pair",
      kind: "stereoPair",
      address: "192.168.1.14",
      memberCount: 2,
      unsupportedReason: null,
    },
    {
      id: "AA:BB:CC:DD:EE:01",
      name: "Kitchen",
      model: "AudioAccessory5,1",
      modelName: "HomePod mini",
      kind: "single",
      address: "192.168.1.20",
      memberCount: 1,
      unsupportedReason: null,
    },
  ];
  let status: EngineStatus = { ...idleStatus };
  let settings: Settings = {
    lastSpeakerIds: [],
    sourceKind: "system",
    lastAppExe: null,
    volume: 25,
    autoReconnect: true,
    closeToTray: true,
  };
  const statusListeners = new Set<(s: EngineStatus) => void>();
  const emit = (patch: Partial<EngineStatus>) => {
    status = { ...status, ...patch };
    statusListeners.forEach((l) => l(status));
  };
  return {
    appInfo: async () => ({
      version: "dev",
      platform: "browser",
      appCaptureSupported: true,
      captureSupported: true,
      logDir: "",
    }),
    speakers: async () => speakers,
    status: async () => status,
    settings: async () => settings,
    saveSettings: async (s) => {
      settings = s;
    },
    listApps: async () => [
      { pid: 100, name: "Spotify", exe: "Spotify.exe", active: true },
      { pid: 200, name: "Google Chrome", exe: "chrome.exe", active: false },
    ],
    connect: async (speakerIds, source) => {
      emit({ state: { kind: "connecting" }, speakerIds, error: null });
      setTimeout(
        () =>
          emit({
            state: { kind: "streaming" },
            source: source.kind === "system" ? "System audio" : `${source.exe} (app audio)`,
          }),
        900,
      );
    },
    disconnect: async () => emit({ state: { kind: "idle" }, source: null }),
    setVolume: async (volume) => emit({ volume }),
    pause: async () => emit({ state: { kind: "paused" } }),
    resume: async () => emit({ state: { kind: "streaming" } }),
    setAutoReconnect: async (autoReconnect) => emit({ autoReconnect }),
    level: async () => (status.state.kind === "streaming" ? Math.random() * 0.6 : 0),
    openLogDir: async () => {},
    fixFirewall: async () => {},
    quit: async () => {},
    onSpeakers: async () => () => {},
    onStatus: async (cb) => {
      statusListeners.add(cb);
      return () => statusListeners.delete(cb);
    },
  };
}

let instance: Promise<Api> | null = null;

export function api(): Promise<Api> {
  instance ??= inTauri ? tauriApi() : Promise.resolve(mockApi());
  return instance;
}
