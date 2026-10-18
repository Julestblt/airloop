// Presentational pieces of the island: the collapsed pill and the expanded
// panel (main view and settings view). All state lives in App.tsx.

import type { ReactNode } from "react";
import type { AppInfo, AudioApp, EngineStatus, Settings, Speaker } from "../lib/types";
import {
  AddLine,
  AirplayFill,
  AppstoreLine,
  CloseLine,
  ComputerLine,
  DeviceIcon,
  FolderOpenLine,
  LoadingLine,
  PauseFill,
  PlayFill,
  PowerLine,
  RefreshLine,
  Settings3Line,
  ShieldLine,
  UpLine,
  VolumeFill,
  VolumeOffFill,
  WarningLine,
} from "./icons";

export function cn(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

/** Four bars that follow the live audio level. */
export function LevelBars({ level, className }: { level: number; className?: string }) {
  const shape = [0.55, 1, 0.75, 0.45];
  const l = Math.min(1, Math.sqrt(level));
  return (
    <span aria-hidden className={cn("flex h-4 items-center gap-[3px]", className)}>
      {shape.map((k, i) => (
        <span
          key={i}
          className="w-[3px] rounded-full bg-accent transition-[height] duration-100"
          style={{ height: `${Math.max(3, Math.round(16 * l * k))}px` }}
        />
      ))}
    </span>
  );
}

function IconButton({
  label,
  onClick,
  children,
  className,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  className?: string;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-raised text-fg transition hover:bg-raised-hover",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
        className,
      )}
    >
      {children}
    </button>
  );
}

function Chip({
  selected,
  disabled,
  onClick,
  icon,
  children,
  title,
}: {
  selected: boolean;
  disabled?: boolean;
  onClick?: () => void;
  icon?: ReactNode;
  children: ReactNode;
  title?: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      disabled={disabled}
      title={title}
      onClick={onClick}
      className={cn(
        "flex h-9 max-w-44 items-center gap-2 rounded-full pr-3.5 pl-3 text-[13px] font-medium transition",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
        selected ? "bg-fg text-black" : "bg-raised text-fg hover:bg-raised-hover",
        disabled && !selected && "opacity-40",
        disabled && "cursor-default",
      )}
    >
      {icon}
      <span className="truncate">{children}</span>
    </button>
  );
}

// ---------------------------------------------------------------- pill

export function Pill({
  status,
  speakers,
  level,
  onExpand,
}: {
  status: EngineStatus;
  speakers: Speaker[];
  level: number;
  onExpand: () => void;
}) {
  const active = speakers.filter((s) => status.speakerIds.includes(s.id));
  const first = active[0];
  const name =
    active.length > 1 ? `${first?.name ?? ""} +${active.length - 1}` : (first?.name ?? "Airloop");
  let lead: ReactNode = <AirplayFill aria-hidden width={18} height={18} />;
  let label = "Airloop";
  let trail: ReactNode = <span className="text-xs text-muted">Not connected</span>;
  switch (status.state.kind) {
    case "streaming":
      lead = first ? <DeviceIcon speaker={first} size={18} /> : lead;
      label = name;
      trail = <LevelBars level={level} />;
      break;
    case "paused":
      lead = first ? <DeviceIcon speaker={first} size={18} /> : lead;
      label = name;
      trail = <PauseFill aria-hidden width={14} height={14} className="text-muted" />;
      break;
    case "connecting":
    case "reconnecting":
      lead = <LoadingLine aria-hidden width={18} height={18} className="animate-spin" />;
      label = status.state.kind === "connecting" ? "Connecting…" : "Reconnecting…";
      trail = <span className="truncate text-xs text-muted">{name}</span>;
      break;
    case "error":
      lead = <WarningLine aria-hidden width={18} height={18} className="text-danger" />;
      label = "Couldn't connect";
      trail = <span className="text-xs text-muted">Open</span>;
      break;
  }
  return (
    <button
      type="button"
      onClick={onExpand}
      aria-label={`Open Airloop — ${label}`}
      className="flex h-10 w-[280px] items-center gap-2.5 rounded-full bg-black px-4 text-fg focus-visible:outline-2 focus-visible:outline-accent"
    >
      <span className="flex w-7 shrink-0 justify-center">{lead}</span>
      <span className="min-w-0 flex-1 truncate text-left text-[13px] font-medium">{label}</span>
      <span className="flex max-w-24 shrink-0 justify-end">{trail}</span>
    </button>
  );
}

// ---------------------------------------------------------------- panel

export interface PanelProps {
  info: AppInfo | null;
  status: EngineStatus;
  speakers: Speaker[];
  selected: string[];
  onToggleSpeaker: (id: string) => void;
  settings: Settings;
  apps: AudioApp[];
  onPickSystem: () => void;
  onPickApp: (app: AudioApp) => void;
  onRefreshApps: () => void;
  volume: number;
  onVolume: (v: number) => void;
  level: number;
  primaryLabel: string;
  canConnect: boolean;
  onPrimary: () => void;
  onPauseResume: () => void;
  onCollapse: () => void;
  onOpenSettings: () => void;
  fixing: boolean;
  fixError: string | null;
  onFixFirewall: () => void;
}

function Header({
  icon,
  title,
  subtitle,
  right,
}: {
  icon: ReactNode;
  title: string;
  subtitle: ReactNode;
  right: ReactNode;
}) {
  return (
    <div className="flex items-center gap-3.5">
      <div
        aria-hidden
        className="flex h-14 w-14 shrink-0 items-center justify-center rounded-2xl bg-raised text-accent"
      >
        {icon}
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[17px] font-semibold">{title}</div>
        <div className="truncate text-[13px] text-muted">{subtitle}</div>
      </div>
      {right}
    </div>
  );
}

function VolumeRow({ volume, onVolume }: { volume: number; onVolume: (v: number) => void }) {
  const Icon = volume === 0 ? VolumeOffFill : VolumeFill;
  return (
    <div className="flex items-center gap-3">
      <Icon aria-hidden width={20} height={20} className="shrink-0 text-muted" />
      <input
        type="range"
        min={0}
        max={100}
        value={volume}
        aria-label="Speaker volume"
        onChange={(e) => onVolume(Number(e.target.value))}
        className="vol flex-1"
        style={{ ["--fill" as string]: `${volume}%` }}
      />
      <span className="w-8 text-right font-mono text-[13px] text-muted tabular-nums">{volume}</span>
    </div>
  );
}

export function Panel(p: PanelProps) {
  const { status } = p;
  const kind = status.state.kind;
  const live = kind === "streaming" || kind === "paused";
  const busy = kind === "connecting" || kind === "reconnecting";
  const locked = live || busy;
  const active = p.speakers.filter((s) => status.speakerIds.includes(s.id));
  const first = active[0];
  const appCapture = p.info?.appCaptureSupported ?? false;
  const appSelected = p.settings.sourceKind === "app";
  // Show the remembered app even if it isn't playing right now.
  const appChips = [...p.apps];
  if (p.settings.lastAppExe && !appChips.some((a) => a.exe === p.settings.lastAppExe)) {
    appChips.unshift({
      pid: 0,
      exe: p.settings.lastAppExe,
      name: p.settings.lastAppExe.replace(/\.exe$/i, ""),
      active: false,
    });
  }

  const header =
    live && first ? (
      <Header
        icon={<DeviceIcon speaker={first} size={30} />}
        title={active.length > 1 ? active.map((s) => s.name).join(" + ") : first.name}
        subtitle={active.length > 1 ? `${active.length} speakers` : first.modelName}
        right={
          <div className="flex items-center gap-3">
            {kind === "streaming" && <LevelBars level={p.level} className="h-5" />}
            <IconButton label="Collapse" onClick={p.onCollapse}>
              <UpLine width={20} height={20} aria-hidden />
            </IconButton>
          </div>
        }
      />
    ) : (
      <Header
        icon={
          busy ? (
            <LoadingLine width={26} height={26} className="animate-spin" />
          ) : (
            <AirplayFill width={26} height={26} />
          )
        }
        title={busy ? (kind === "connecting" ? "Connecting…" : "Reconnecting…") : "Airloop"}
        subtitle={
          busy ? active.map((s) => s.name).join(" + ") || "…" : "Stream this PC to your speakers"
        }
        right={
          <div className="flex items-center gap-2">
            <IconButton label="Settings" onClick={p.onOpenSettings}>
              <Settings3Line width={20} height={20} aria-hidden />
            </IconButton>
            <IconButton label="Collapse" onClick={p.onCollapse}>
              <UpLine width={20} height={20} aria-hidden />
            </IconButton>
          </div>
        }
      />
    );

  return (
    <div className="flex flex-col gap-4">
      {header}

      {status.error && !live && (
        <div
          role="alert"
          className="flex gap-3 rounded-2xl bg-danger/12 p-3 text-[13px] leading-snug"
        >
          <WarningLine aria-hidden width={18} height={18} className="mt-px shrink-0 text-danger" />
          <div className="flex-1 space-y-2">
            <p>{status.error.message}</p>
            {status.error.kind === "firewallBlocked" && (
              <button
                type="button"
                disabled={p.fixing}
                onClick={p.onFixFirewall}
                className="flex h-9 items-center gap-2 rounded-full bg-fg px-3.5 text-[13px] font-semibold text-black disabled:opacity-60"
              >
                <ShieldLine aria-hidden width={16} height={16} />
                {p.fixing ? "Waiting for Windows…" : "Allow in firewall"}
              </button>
            )}
            {p.fixError && <p className="text-danger">{p.fixError}</p>}
          </div>
        </div>
      )}

      <div
        role="radiogroup"
        aria-label="Audio source"
        className="flex flex-wrap items-center gap-2"
      >
        {(!locked || !appSelected) && (
          <Chip
            selected={!appSelected}
            disabled={locked}
            onClick={p.onPickSystem}
            icon={<ComputerLine aria-hidden width={16} height={16} />}
          >
            Whole PC
          </Chip>
        )}
        {appCapture &&
          appChips
            .filter((a) => !locked || (appSelected && p.settings.lastAppExe === a.exe))
            .slice(0, 4)
            .map((a) => (
              <Chip
                key={a.exe}
                selected={appSelected && p.settings.lastAppExe === a.exe}
                disabled={locked}
                onClick={() => p.onPickApp(a)}
                title={a.active ? `${a.name} is playing sound` : a.name}
                icon={<AppstoreLine aria-hidden width={16} height={16} />}
              >
                {a.name}
              </Chip>
            ))}
        {appCapture && !locked && (
          <button
            type="button"
            aria-label="Refresh the list of apps playing sound"
            title="Refresh apps"
            onClick={p.onRefreshApps}
            className="flex h-9 w-9 items-center justify-center rounded-full border border-dashed border-line text-muted transition hover:text-fg"
          >
            {appChips.length === 0 ? (
              <AddLine aria-hidden width={16} height={16} />
            ) : (
              <RefreshLine aria-hidden width={16} height={16} />
            )}
          </button>
        )}
      </div>

      {!live && (
        <ul aria-label="Speakers" className="flex max-h-56 flex-col gap-1 overflow-y-auto">
          {p.speakers.length === 0 && (
            <li className="flex items-center gap-3 rounded-2xl bg-raised px-4 py-3.5 text-[13px] text-muted">
              <LoadingLine aria-hidden width={18} height={18} className="animate-spin" />
              Looking for speakers on your network…
            </li>
          )}
          {p.speakers.map((s) => {
            const on = (busy ? status.speakerIds : p.selected).includes(s.id);
            const unsupported = s.unsupportedReason !== null;
            return (
              <li key={s.id}>
                <button
                  type="button"
                  aria-pressed={on}
                  disabled={locked || unsupported}
                  title={s.unsupportedReason ?? undefined}
                  onClick={() => p.onToggleSpeaker(s.id)}
                  className={cn(
                    "flex w-full items-center gap-3 rounded-2xl px-3 py-2.5 text-left transition",
                    "focus-visible:outline-2 focus-visible:outline-accent",
                    on ? "bg-raised" : "hover:bg-raised/60",
                    unsupported && "opacity-40",
                  )}
                >
                  <span
                    className={cn(
                      "flex h-10 w-12 items-center justify-center rounded-xl",
                      on ? "text-accent" : "text-muted",
                    )}
                  >
                    <DeviceIcon speaker={s} size={24} />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[15px] font-medium">{s.name}</span>
                    <span className="block truncate text-[12px] text-muted">
                      {s.unsupportedReason ?? s.modelName}
                    </span>
                  </span>
                  <span
                    aria-hidden
                    className={cn(
                      "flex h-5 w-5 items-center justify-center rounded-full border-2 transition",
                      on ? "border-accent bg-accent" : "border-line",
                    )}
                  >
                    {on && <span className="h-1.5 w-1.5 rounded-full bg-black" />}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <VolumeRow volume={p.volume} onVolume={p.onVolume} />

      <div className="flex gap-2.5">
        {live && (
          <button
            type="button"
            onClick={p.onPauseResume}
            className="flex h-11 flex-1 items-center justify-center gap-2 rounded-full bg-raised text-[14px] font-medium transition hover:bg-raised-hover"
          >
            {kind === "paused" ? (
              <PlayFill aria-hidden width={16} height={16} />
            ) : (
              <PauseFill aria-hidden width={16} height={16} />
            )}
            {kind === "paused" ? "Resume" : "Pause"}
          </button>
        )}
        <button
          type="button"
          disabled={!locked && !p.canConnect}
          onClick={p.onPrimary}
          className={cn(
            "flex h-11 flex-1 items-center justify-center gap-2 rounded-full text-[14px] font-semibold transition",
            locked
              ? "bg-raised text-fg hover:bg-raised-hover"
              : "bg-accent text-black hover:brightness-110",
            "disabled:cursor-default disabled:opacity-40",
          )}
        >
          {locked ? (
            <PowerLine aria-hidden width={16} height={16} />
          ) : (
            <PlayFill aria-hidden width={16} height={16} />
          )}
          {p.primaryLabel}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- settings

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-4 rounded-2xl bg-raised px-4 py-3">
      <div className="flex-1">
        <p className="text-[14px] font-medium">{label}</p>
        <p className="text-[12px] text-muted">{hint}</p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={cn(
          "relative h-6 w-10 shrink-0 rounded-full transition",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
          checked ? "bg-accent" : "bg-line",
        )}
      >
        <span
          className={cn(
            "absolute top-1 h-4 w-4 rounded-full bg-black transition-all",
            checked ? "left-5" : "left-1 bg-fg",
          )}
        />
      </button>
    </div>
  );
}

export function SettingsView({
  info,
  status,
  settings,
  onChange,
  onBack,
  onOpenLogs,
  onQuit,
}: {
  info: AppInfo | null;
  status: EngineStatus;
  settings: Settings;
  onChange: (s: Settings) => void;
  onBack: () => void;
  onOpenLogs: () => void;
  onQuit: () => void;
}) {
  const d = status.diagnostics;
  const live = status.state.kind === "streaming" || status.state.kind === "paused";
  const rows: [string, string][] = [
    ["State", status.state.kind],
    ["Buffer", live ? `${d.bufferMs.toFixed(0)} ms` : "—"],
    ["Clock drift", live ? `${d.driftPpm.toFixed(1)} ppm` : "—"],
    ["Underruns", String(d.bufferUnderruns)],
    ["Dropped", String(d.droppedChunks)],
    ["Packets", String(d.speakers.reduce((n, s) => n + s.packetsSent, 0))],
  ];
  return (
    <div className="flex flex-col gap-4" role="dialog" aria-label="Settings">
      <div className="flex items-center justify-between">
        <h1 className="text-[17px] font-semibold">Settings</h1>
        <IconButton label="Close settings" onClick={onBack}>
          <CloseLine width={20} height={20} aria-hidden />
        </IconButton>
      </div>
      <Toggle
        label="Reconnect automatically"
        hint="Retry if a speaker drops off the network."
        checked={settings.autoReconnect}
        onChange={(v) => onChange({ ...settings, autoReconnect: v })}
      />
      <dl className="grid grid-cols-3 gap-2">
        {rows.map(([k, v]) => (
          <div key={k} className="rounded-2xl bg-raised px-3 py-2">
            <dt className="text-[11px] text-muted">{k}</dt>
            <dd className="truncate font-mono text-[13px] tabular-nums">{v}</dd>
          </div>
        ))}
      </dl>
      {status.error && (
        <pre className="max-h-24 overflow-auto rounded-2xl bg-raised p-3 font-mono text-[11px] whitespace-pre-wrap text-muted select-text">
          {status.error.detail}
        </pre>
      )}
      <div className="flex gap-2.5">
        <button
          type="button"
          onClick={onOpenLogs}
          className="flex h-11 flex-1 items-center justify-center gap-2 rounded-full bg-raised text-[14px] font-medium transition hover:bg-raised-hover"
        >
          <FolderOpenLine aria-hidden width={16} height={16} /> Logs
        </button>
        <button
          type="button"
          onClick={onQuit}
          className="flex h-11 flex-1 items-center justify-center gap-2 rounded-full bg-raised text-[14px] font-medium text-danger transition hover:bg-raised-hover"
        >
          <PowerLine aria-hidden width={16} height={16} /> Quit Airloop
        </button>
      </div>
      <p className="text-center text-[11px] text-muted">
        Airloop {info?.version ?? ""} · GPL-3.0 · Icons: MingCute (Apache-2.0) · Not affiliated with
        Apple
      </p>
    </div>
  );
}
