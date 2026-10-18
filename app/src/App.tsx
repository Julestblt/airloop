import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Panel, Pill, SettingsView, cn } from "./components/Island";
import { api, idleStatus } from "./lib/api";
import { fitWindow, onExpandRequest, onWindowBlur, showWindow } from "./lib/island";
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

type View = "pill" | "panel" | "settings";

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [speakers, setSpeakers] = useState<Speaker[]>([]);
  const [status, setStatus] = useState<EngineStatus>(idleStatus);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [selected, setSelected] = useState<string[]>([]);
  const [apps, setApps] = useState<AudioApp[]>([]);
  const [volume, setVolumeState] = useState(25);
  const [level, setLevel] = useState(0);
  const [view, setView] = useState<View>("panel");
  const [fixing, setFixing] = useState(false);
  const [fixError, setFixError] = useState<string | null>(null);

  const active = isActive(status);

  // Initial load + subscriptions (subscribe first: the backend only emits on change).
  useEffect(() => {
    let cleanup: (() => void)[] = [];
    let cancelled = false;
    (async () => {
      const a = await api();
      const subs = [
        await a.onSpeakers(setSpeakers),
        await a.onStatus(setStatus),
        await onExpandRequest(() => setView("panel")),
      ];
      if (cancelled) {
        subs.forEach((f) => f());
        return;
      }
      cleanup = subs;
      const [i, sp, st, se] = await Promise.all([
        a.appInfo(),
        a.speakers(),
        a.status(),
        a.settings(),
      ]);
      if (cancelled) return;
      setInfo(i);
      setSpeakers((cur) => (cur.length > 0 ? cur : sp));
      setStatus(st);
      setSettings(se);
      setSelected(isActive(st) ? st.speakerIds : se.lastSpeakerIds);
      setVolumeState(se.volume);
    })();
    return () => {
      cancelled = true;
      cleanup.forEach((f) => f());
    };
  }, []);

  // Dynamic-island behaviour: clicking anywhere else folds it back to a pill.
  useEffect(() => {
    let off = () => {};
    void onWindowBlur(() => setView("pill")).then((f) => (off = f));
    return () => off();
  }, []);

  // Safety net: while no speaker is known, re-read the list every 2 s.
  const noSpeakers = speakers.length === 0;
  useEffect(() => {
    if (!noSpeakers) return;
    const id = setInterval(async () => {
      const list = await (await api()).speakers();
      if (list.length > 0) setSpeakers(list);
    }, 2000);
    return () => clearInterval(id);
  }, [noSpeakers]);

  const refreshApps = useCallback(async () => {
    try {
      setApps(await (await api()).listApps());
    } catch {
      setApps([]);
    }
  }, []);

  // Load the app list whenever the panel opens.
  const panelOpen = view === "panel";
  useEffect(() => {
    if (!panelOpen) return;
    let cancelled = false;
    api()
      .then((a) => a.listApps())
      .then((list) => !cancelled && setApps(list))
      .catch(() => !cancelled && setApps([]));
    return () => {
      cancelled = true;
    };
  }, [panelOpen]);

  // Live level for the bars.
  const streaming = status.state.kind === "streaming";
  useEffect(() => {
    if (!streaming) return;
    const id = setInterval(async () => setLevel(await (await api()).level()), 80);
    return () => clearInterval(id);
  }, [streaming]);
  const shownLevel = streaming ? level : 0;

  // Size the window to the island and keep it pinned to the top centre.
  const islandRef = useRef<HTMLDivElement>(null);
  const shown = useRef(false);
  useLayoutEffect(() => {
    const el = islandRef.current;
    if (!el) return;
    const fit = () => {
      const r = el.getBoundingClientRect();
      void fitWindow(r.width, r.height).then(() => {
        if (!shown.current) {
          shown.current = true;
          void showWindow();
        }
      });
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

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
  const needsApp = settings.sourceKind === "app" && !settings.lastAppExe;
  const canConnect = availableSelected.length > 0 && !needsApp;

  const connect = async () => {
    const a = await api();
    const source =
      settings.sourceKind === "app" && settings.lastAppExe
        ? { kind: "app" as const, pid: selectedApp?.pid ?? 0, exe: settings.lastAppExe }
        : { kind: "system" as const };
    await a.setVolume(volume);
    await persist({ ...settings, lastSpeakerIds: availableSelected, volume });
    await a.connect(availableSelected, source);
  };

  const fixFirewall = async () => {
    setFixing(true);
    setFixError(null);
    try {
      await (await api()).fixFirewall();
      await connect();
    } catch (e) {
      setFixError(String(e));
    } finally {
      setFixing(false);
    }
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
        if (availableSelected.length === 0) return "Choose a speaker";
        if (needsApp) return "Choose an app";
        return availableSelected.length > 1
          ? `Play on ${availableSelected.length} speakers`
          : "Play";
    }
  })();

  return (
    <div className="flex justify-center" style={{ padding: "8px 28px 34px" }}>
      <div
        ref={islandRef}
        className={cn(
          "island bg-island text-fg shadow-island",
          view === "pill"
            ? "w-[280px] rounded-full"
            : "w-[540px] rounded-[34px] px-5 pt-[18px] pb-5",
        )}
      >
        {view === "pill" && (
          <Pill
            status={status}
            speakers={speakers}
            level={shownLevel}
            onExpand={() => setView("panel")}
          />
        )}
        {view === "panel" && (
          <Panel
            info={info}
            status={status}
            speakers={speakers}
            selected={selected}
            onToggleSpeaker={toggleSpeaker}
            settings={settings}
            apps={apps}
            onPickSystem={() => void persist({ ...settings, sourceKind: "system" })}
            onPickApp={(a) => void persist({ ...settings, sourceKind: "app", lastAppExe: a.exe })}
            onRefreshApps={() => void refreshApps()}
            volume={volume}
            onVolume={onVolume}
            level={shownLevel}
            primaryLabel={primaryLabel}
            canConnect={canConnect}
            onPrimary={() => void primary()}
            onPauseResume={async () => {
              const a = await api();
              if (status.state.kind === "paused") await a.resume();
              else await a.pause();
            }}
            onCollapse={() => setView("pill")}
            onOpenSettings={() => setView("settings")}
            fixing={fixing}
            fixError={fixError}
            onFixFirewall={() => void fixFirewall()}
          />
        )}
        {view === "settings" && (
          <SettingsView
            info={info}
            status={status}
            settings={settings}
            onChange={async (s) => {
              await persist(s);
              if (s.autoReconnect !== settings.autoReconnect) {
                await (await api()).setAutoReconnect(s.autoReconnect);
              }
            }}
            onBack={() => setView("panel")}
            onOpenLogs={async () => (await api()).openLogDir()}
            onQuit={async () => (await api()).quit()}
          />
        )}
      </div>
    </div>
  );
}
