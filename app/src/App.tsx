import {
  AlertCircle,
  Pause,
  Play,
  Power,
  Settings2,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SettingsPanel } from "./components/SettingsPanel";
import { SourcePicker } from "./components/SourcePicker";
import { SpeakerList } from "./components/SpeakerList";
import { StatusPill } from "./components/StatusPill";
import { Button, SectionTitle, Slider, cn } from "./components/ui";
import { api, idleStatus } from "./lib/api";
import type { AppInfo, AudioApp, EngineStatus, Settings, Speaker } from "./lib/types";

const defaultSettings: Settings = {
  lastSpeakerIds: [],
  sourceKind: "system",
  lastAppExe: null,
  volume: 25,
  autoReconnect: true,
  closeToTray: true,
};

/** Whether a session exists (selection is locked). */
export function isActive(status: EngineStatus): boolean {
  const k = status.state.kind;
  return k === "connecting" || k === "streaming" || k === "paused" || k === "reconnecting";
}

function LevelMeter({ level }: { level: number }) {
  const bars = 12;
  const lit = Math.round(Math.min(1, Math.sqrt(level)) * bars);
  return (
    <div className="flex h-3 items-end gap-0.5" aria-hidden>
      {Array.from({ length: bars }, (_, i) => (
        <span
          key={i}
          className={cn(
            "w-1 rounded-sm transition-colors duration-75",
            i < lit ? (i > bars - 3 ? "bg-warn" : "bg-ok") : "bg-border",
          )}
          style={{ height: `${30 + (i / bars) * 70}%` }}
        />
      ))}
    </div>
  );
}

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [speakers, setSpeakers] = useState<Speaker[]>([]);
  const [status, setStatus] = useState<EngineStatus>(idleStatus);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [selected, setSelected] = useState<string[]>([]);
  const [apps, setApps] = useState<AudioApp[]>([]);
  const [volume, setVolumeState] = useState(25);
  const [level, setLevel] = useState(0);
  const [showSettings, setShowSettings] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const active = isActive(status);

  // Initial load + subscriptions.
  useEffect(() => {
    let cleanup: (() => void)[] = [];
    let cancelled = false;
    (async () => {
      const a = await api();
      const [i, sp, st, se] = await Promise.all([
        a.appInfo(),
        a.speakers(),
        a.status(),
        a.settings(),
      ]);
      if (cancelled) return;
      setInfo(i);
      setSpeakers(sp);
      setStatus(st);
      setSettings(se);
      setSelected(isActive(st) ? st.speakerIds : se.lastSpeakerIds);
      setVolumeState(se.volume);
      setLoaded(true);
      cleanup = [await a.onSpeakers(setSpeakers), await a.onStatus(setStatus)];
    })();
    return () => {
      cancelled = true;
      cleanup.forEach((f) => f());
    };
  }, []);

  const refreshApps = useCallback(async () => {
    try {
      setApps(await (await api()).listApps());
    } catch {
      setApps([]);
    }
  }, []);

  useEffect(() => {
    if (settings.sourceKind !== "app") return;
    let cancelled = false;
    api()
      .then((a) => a.listApps())
      .then((list) => !cancelled && setApps(list))
      .catch(() => !cancelled && setApps([]));
    return () => {
      cancelled = true;
    };
  }, [settings.sourceKind]);

  // Level meter polling while audio flows.
  const streaming = status.state.kind === "streaming";
  useEffect(() => {
    if (!streaming) return;
    const id = setInterval(async () => setLevel(await (await api()).level()), 80);
    return () => clearInterval(id);
  }, [streaming]);
  const shownLevel = streaming ? level : 0;

  const persist = useCallback(async (next: Settings) => {
    setSettings(next);
    await (await api()).saveSettings(next);
  }, []);

  const selectedApp = useMemo(
    () => apps.find((a) => a.exe === settings.lastAppExe) ?? null,
    [apps, settings.lastAppExe],
  );

  const toggleSpeaker = (id: string) =>
    setSelected((cur) => (cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id]));

  // Volume: update UI immediately, send at most every 80 ms.
  const volumeTimer = useRef<number | null>(null);
  const onVolume = (v: number) => {
    setVolumeState(v);
    if (volumeTimer.current) window.clearTimeout(volumeTimer.current);
    volumeTimer.current = window.setTimeout(async () => {
      await (await api()).setVolume(v);
      await persist({ ...settings, volume: v });
    }, 80);
  };

  const availableSelected = selected.filter((id) => speakers.some((s) => s.id === id));
  const needsApp = settings.sourceKind === "app" && !selectedApp;
  const canConnect = availableSelected.length > 0 && !needsApp;

  const connect = async () => {
    const a = await api();
    const source =
      settings.sourceKind === "app" && selectedApp
        ? { kind: "app" as const, pid: selectedApp.pid, exe: selectedApp.exe }
        : { kind: "system" as const };
    await a.setVolume(volume);
    await persist({ ...settings, lastSpeakerIds: availableSelected, volume });
    await a.connect(availableSelected, source);
  };

  const primary = async () => {
    const a = await api();
    if (active) await a.disconnect();
    else await connect();
  };

  const primaryLabel = (() => {
    switch (status.state.kind) {
      case "connecting":
        return "Cancel";
      case "streaming":
      case "paused":
      case "reconnecting":
        return "Disconnect";
      default:
        if (availableSelected.length === 0) return "Select a speaker";
        if (needsApp) return "Choose an app";
        return availableSelected.length > 1
          ? `Play on ${availableSelected.length} speakers`
          : "Connect";
    }
  })();

  const VolumeIcon = volume === 0 ? VolumeX : volume < 50 ? Volume1 : Volume2;

  return (
    <div className="relative flex h-full flex-col">
      <header className="flex items-center justify-between px-5 pt-5 pb-3">
        <div className="flex items-center gap-2.5">
          <img src="/logo.svg" alt="" className="h-7 w-7" />
          <h1 className="text-lg font-semibold tracking-tight">Airloop</h1>
        </div>
        <div className="flex items-center gap-1">
          <StatusPill state={status.state} />
          <Button variant="ghost" onClick={() => setShowSettings(true)} aria-label="Settings">
            <Settings2 className="h-4 w-4" />
          </Button>
        </div>
      </header>

      <main className="flex-1 space-y-5 overflow-y-auto px-5 pb-4">
        {status.error && status.state.kind !== "streaming" && (
          <div
            role="alert"
            className="flex gap-3 rounded-[var(--radius-card)] border border-danger/30 bg-danger/10 p-3 text-sm"
          >
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
            <p>{status.error.message}</p>
          </div>
        )}

        <section>
          <SectionTitle>Play from</SectionTitle>
          <SourcePicker
            kind={settings.sourceKind}
            onKind={(k) => void persist({ ...settings, sourceKind: k })}
            apps={apps}
            selectedExe={settings.lastAppExe}
            onSelectApp={(a) => void persist({ ...settings, lastAppExe: a.exe })}
            onRefresh={() => void refreshApps()}
            appCaptureSupported={info?.appCaptureSupported ?? false}
            locked={active}
          />
          {info && !info.captureSupported && (
            <p className="mt-2 px-1 text-xs text-warn">
              Audio capture is only available on Windows. Use the test tone build for protocol
              tests.
            </p>
          )}
        </section>

        <section>
          <SectionTitle>Speakers</SectionTitle>
          {loaded && (
            <SpeakerList
              speakers={speakers}
              selected={active ? status.speakerIds : selected}
              onToggle={toggleSpeaker}
              locked={active}
              connectedIds={status.state.kind === "streaming" ? status.speakerIds : []}
            />
          )}
        </section>
      </main>

      <footer className="space-y-3 border-t border-border bg-surface px-5 py-4">
        <div className="flex items-center gap-3">
          <VolumeIcon className="h-4 w-4 shrink-0 text-muted" />
          <Slider value={volume} onChange={onVolume} label="Speaker volume" />
          <span className="w-8 text-right text-xs text-muted tabular-nums">{volume}</span>
        </div>
        <div className="flex items-center gap-2">
          {(status.state.kind === "streaming" || status.state.kind === "paused") && (
            <Button
              aria-label={status.state.kind === "paused" ? "Resume" : "Pause"}
              onClick={async () => {
                const a = await api();
                if (status.state.kind === "paused") await a.resume();
                else await a.pause();
              }}
            >
              {status.state.kind === "paused" ? (
                <Play className="h-4 w-4" />
              ) : (
                <Pause className="h-4 w-4" />
              )}
            </Button>
          )}
          <Button
            variant={active ? "danger" : "primary"}
            className="h-10 flex-1"
            disabled={!active && !canConnect}
            onClick={() => void primary()}
          >
            <Power className="h-4 w-4" /> {primaryLabel}
          </Button>
          {status.state.kind === "streaming" && <LevelMeter level={shownLevel} />}
        </div>
      </footer>

      {showSettings && (
        <SettingsPanel
          settings={settings}
          onChange={async (s) => {
            await persist(s);
            if (s.autoReconnect !== settings.autoReconnect)
              await (await api()).setAutoReconnect(s.autoReconnect);
          }}
          status={status}
          info={info}
          onClose={() => setShowSettings(false)}
          onOpenLogs={async () => (await api()).openLogDir()}
        />
      )}
    </div>
  );
}
