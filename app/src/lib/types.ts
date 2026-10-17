// Mirrors the serde types in crates/engine and app/src-tauri. Keep in sync.

export type SpeakerKind = "single" | "stereoPair";

export interface Speaker {
  id: string;
  name: string;
  model: string;
  modelName: string;
  kind: SpeakerKind;
  address: string | null;
  memberCount: number;
  unsupportedReason: string | null;
}

export type EngineState =
  | { kind: "idle" }
  | { kind: "connecting" }
  | { kind: "streaming" }
  | { kind: "paused" }
  | { kind: "reconnecting"; attempt: number }
  | { kind: "error" };

export type ErrorKind =
  | "unreachable"
  | "accessDenied"
  | "timingPortsBusy"
  | "protocol"
  | "speakerGone"
  | "capture"
  | "internal";

export interface UiError {
  kind: ErrorKind;
  message: string;
  detail: string;
}

export interface SpeakerStats {
  speakerId: string;
  packetsSent: number;
  underruns: number;
}

export interface Diagnostics {
  bufferMs: number;
  driftPpm: number;
  bufferUnderruns: number;
  droppedChunks: number;
  speakers: SpeakerStats[];
}

export interface EngineStatus {
  state: EngineState;
  speakerIds: string[];
  volume: number;
  source: string | null;
  error: UiError | null;
  autoReconnect: boolean;
  diagnostics: Diagnostics;
}

export interface AudioApp {
  pid: number;
  name: string;
  exe: string;
  active: boolean;
}

export type SourceChoice = { kind: "system" } | { kind: "app"; pid: number; exe: string };

export interface Settings {
  lastSpeakerIds: string[];
  sourceKind: "system" | "app";
  lastAppExe: string | null;
  volume: number;
  autoReconnect: boolean;
  closeToTray: boolean;
}

export interface AppInfo {
  version: string;
  platform: string;
  appCaptureSupported: boolean;
  captureSupported: boolean;
  logDir: string;
}
