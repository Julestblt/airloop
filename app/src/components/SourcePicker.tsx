import { AppWindow, MonitorSpeaker, RefreshCw } from "lucide-react";
import type { AudioApp } from "../lib/types";
import { Button, cn } from "./ui";

export function SourcePicker({
  kind,
  onKind,
  apps,
  selectedExe,
  onSelectApp,
  onRefresh,
  appCaptureSupported,
  locked,
}: {
  kind: "system" | "app";
  onKind: (k: "system" | "app") => void;
  apps: AudioApp[];
  selectedExe: string | null;
  onSelectApp: (app: AudioApp) => void;
  onRefresh: () => void;
  appCaptureSupported: boolean;
  locked: boolean;
}) {
  const tab = (k: "system" | "app", label: string, Icon: typeof AppWindow, disabled = false) => (
    <button
      role="tab"
      aria-selected={kind === k}
      disabled={locked || disabled}
      onClick={() => onKind(k)}
      className={cn(
        "flex flex-1 items-center justify-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium transition",
        "focus-visible:outline-2 focus-visible:outline-accent disabled:opacity-50",
        kind === k ? "bg-surface text-fg shadow-sm" : "text-muted hover:text-fg",
      )}
    >
      <Icon className="h-4 w-4" /> {label}
    </button>
  );

  return (
    <div>
      <div
        role="tablist"
        aria-label="Audio source"
        className="flex gap-1 rounded-lg bg-surface-2 p-1"
      >
        {tab("system", "Whole PC", MonitorSpeaker)}
        {tab("app", "One app", AppWindow, !appCaptureSupported)}
      </div>
      {!appCaptureSupported && (
        <p className="mt-2 px-1 text-xs text-muted">
          Capturing a single app needs Windows 11 (or Windows 10 build 20348+).
        </p>
      )}
      {kind === "app" && appCaptureSupported && (
        <div className="mt-2 rounded-[var(--radius-card)] border border-border bg-surface">
          <div className="flex items-center justify-between px-3 pt-2 pb-1">
            <span className="text-xs text-muted">Apps playing sound</span>
            <Button
              variant="ghost"
              className="px-2 py-1"
              onClick={onRefresh}
              aria-label="Refresh apps"
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>
          </div>
          {apps.length === 0 ? (
            <p className="px-3 pb-3 text-xs text-muted">
              No app is using audio right now. Start playback, then refresh.
            </p>
          ) : (
            <ul className="max-h-40 overflow-y-auto px-1 pb-1">
              {apps.map((a) => (
                <li key={a.pid}>
                  <button
                    disabled={locked}
                    onClick={() => onSelectApp(a)}
                    aria-pressed={selectedExe === a.exe}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition",
                      selectedExe === a.exe ? "bg-accent/15 text-fg" : "hover:bg-surface-2",
                    )}
                  >
                    <span
                      className={cn("h-2 w-2 rounded-full", a.active ? "bg-ok" : "bg-border")}
                      title={a.active ? "Playing sound" : "Idle"}
                    />
                    <span className="flex-1 truncate">{a.name}</span>
                    <span className="truncate text-xs text-muted">{a.exe}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
