import { FolderOpen, X } from "lucide-react";
import type { AppInfo, EngineStatus, Settings } from "../lib/types";
import { Button, SectionTitle, Switch } from "./ui";

function Row({
  title,
  hint,
  children,
}: {
  title: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-4 px-4 py-3">
      <div className="flex-1">
        <p className="font-medium">{title}</p>
        <p className="text-xs text-muted">{hint}</p>
      </div>
      {children}
    </div>
  );
}

export function SettingsPanel({
  settings,
  onChange,
  status,
  info,
  onClose,
  onOpenLogs,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
  status: EngineStatus;
  info: AppInfo | null;
  onClose: () => void;
  onOpenLogs: () => void;
}) {
  const d = status.diagnostics;
  const active = status.state.kind === "streaming" || status.state.kind === "paused";
  return (
    <div className="absolute inset-0 z-10 flex flex-col bg-bg" role="dialog" aria-label="Settings">
      <header className="flex items-center justify-between px-5 pt-5 pb-3">
        <h1 className="text-lg font-semibold">Settings</h1>
        <Button variant="ghost" onClick={onClose} aria-label="Close settings">
          <X className="h-4 w-4" />
        </Button>
      </header>
      <div className="flex-1 space-y-5 overflow-y-auto px-5 pb-5">
        <section>
          <SectionTitle>Behaviour</SectionTitle>
          <div className="divide-y divide-border rounded-[var(--radius-card)] border border-border bg-surface">
            <Row title="Reconnect automatically" hint="Retry if the speaker drops off the network.">
              <Switch
                label="Reconnect automatically"
                checked={settings.autoReconnect}
                onChange={(v) => onChange({ ...settings, autoReconnect: v })}
              />
            </Row>
            <Row title="Keep running in the tray" hint="Closing the window keeps audio streaming.">
              <Switch
                label="Keep running in the tray"
                checked={settings.closeToTray}
                onChange={(v) => onChange({ ...settings, closeToTray: v })}
              />
            </Row>
          </div>
        </section>

        <section>
          <SectionTitle>Diagnostics</SectionTitle>
          <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-[var(--radius-card)] border border-border bg-border text-sm">
            {[
              ["State", status.state.kind],
              ["Source", status.source ?? "—"],
              ["Buffer", active ? `${d.bufferMs.toFixed(0)} ms` : "—"],
              ["Clock drift", active ? `${d.driftPpm.toFixed(1)} ppm` : "—"],
              ["Buffer underruns", String(d.bufferUnderruns)],
              ["Dropped chunks", String(d.droppedChunks)],
              ...d.speakers.map((s) => [
                `Packets → ${s.speakerId.slice(0, 12)}`,
                `${s.packetsSent} (${s.underruns} underruns)`,
              ]),
            ].map(([k, v]) => (
              <div key={k} className="bg-surface px-3 py-2">
                <dt className="text-xs text-muted">{k}</dt>
                <dd className="truncate font-medium tabular-nums">{v}</dd>
              </div>
            ))}
          </dl>
          {status.error && (
            <pre className="mt-2 rounded-lg bg-surface-2 p-3 text-xs whitespace-pre-wrap text-muted select-text">
              {status.error.detail}
            </pre>
          )}
          <Button className="mt-3 w-full" onClick={onOpenLogs}>
            <FolderOpen className="h-4 w-4" /> Open log folder
          </Button>
        </section>

        <p className="text-center text-xs text-muted">
          Airloop {info?.version ?? ""} · GPL-3.0 · Not affiliated with Apple
        </p>
      </div>
    </div>
  );
}
