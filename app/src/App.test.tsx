import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import App, { isActive } from "./App";
import { Panel, Pill, type PanelProps } from "./components/Island";
import { deviceFamily } from "./components/icons";
import { idleStatus } from "./lib/api";
import type { Settings, Speaker } from "./lib/types";

beforeAll(() => {
  // jsdom has no ResizeObserver; the island only uses it to size the window.
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
});

afterEach(cleanup);

const pair: Speaker = {
  id: "pair-1",
  name: "Living Room",
  model: "AudioAccessory5,1",
  modelName: "HomePod mini stereo pair",
  kind: "stereoPair",
  address: "192.168.1.14",
  memberCount: 2,
  unsupportedReason: null,
};

const settings: Settings = {
  lastSpeakerIds: [],
  sourceKind: "system",
  lastAppExe: null,
  volume: 25,
  autoReconnect: true,
  lowLatency: false,
  closeToTray: true,
};

function panelProps(over: Partial<PanelProps> = {}): PanelProps {
  return {
    info: null,
    status: idleStatus,
    speakers: [pair],
    selected: [],
    onToggleSpeaker: () => {},
    settings,
    apps: [],
    onPickSystem: () => {},
    onPickApp: () => {},
    onRefreshApps: () => {},
    volume: 25,
    onVolume: () => {},
    level: 0,
    primaryLabel: "Play",
    canConnect: true,
    onPrimary: () => {},
    onPauseResume: () => {},
    onCollapse: () => {},
    onOpenSettings: () => {},
    fixing: false,
    fixError: null,
    onFixFirewall: () => {},
    ...over,
  };
}

describe("helpers", () => {
  it("treats connecting/streaming/paused as active", () => {
    expect(isActive(idleStatus)).toBe(false);
    for (const kind of ["connecting", "streaming", "paused"] as const) {
      expect(isActive({ ...idleStatus, state: { kind } })).toBe(true);
    }
    expect(isActive({ ...idleStatus, state: { kind: "error" } })).toBe(false);
  });

  it("picks an icon family from the Apple model id", () => {
    expect(deviceFamily("AudioAccessory5,1")).toBe("homepodMini");
    expect(deviceFamily("AudioAccessory6,1")).toBe("homepod");
    expect(deviceFamily("AppleTV11,1")).toBe("appleTv");
    expect(deviceFamily("MacBookPro18,1")).toBe("laptop");
    expect(deviceFamily("Mac14,2")).toBe("desktop");
    expect(deviceFamily("Sonos")).toBe("other");
  });
});

describe("Pill", () => {
  it("shows the speaker name while streaming", () => {
    render(
      <Pill
        status={{ ...idleStatus, state: { kind: "streaming" }, speakerIds: ["pair-1"] }}
        speakers={[pair]}
        level={0.5}
        onExpand={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: /Living Room/ })).toBeTruthy();
  });

  it("says when it is not connected", () => {
    render(<Pill status={idleStatus} speakers={[pair]} level={0} onExpand={() => {}} />);
    expect(screen.getByText("Not connected")).toBeTruthy();
  });
});

describe("Panel", () => {
  it("shows a searching state when no speaker is known", () => {
    render(<Panel {...panelProps({ speakers: [] })} />);
    expect(screen.getByText(/Looking for speakers/)).toBeTruthy();
  });

  it("renders a stereo pair as one selectable entry", () => {
    const toggled: string[] = [];
    render(<Panel {...panelProps({ onToggleSpeaker: (id: string) => toggled.push(id) })} />);
    expect(screen.getByText("HomePod mini stereo pair")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Living Room/ }));
    expect(toggled).toEqual(["pair-1"]);
  });

  it("disables unsupported speakers", () => {
    render(
      <Panel {...panelProps({ speakers: [{ ...pair, unsupportedReason: "Needs a password" }] })} />,
    );
    const b = screen.getByRole("button", { name: /Living Room/ }) as HTMLButtonElement;
    expect(b.disabled).toBe(true);
  });

  it("offers the firewall fix for a firewall error", () => {
    let fixed = false;
    render(
      <Panel
        {...panelProps({
          status: {
            ...idleStatus,
            state: { kind: "error" },
            error: { kind: "firewallBlocked", message: "Blocked", detail: "" },
          },
          onFixFirewall: () => (fixed = true),
        })}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Allow in firewall/ }));
    expect(fixed).toBe(true);
  });
});

describe("App (mock backend)", () => {
  it("connects to a selected speaker and shows streaming", async () => {
    render(<App />);
    const speaker = await screen.findByRole("button", { name: /Living Room/ });
    const play = screen.getByRole("button", { name: /Choose a speaker/ }) as HTMLButtonElement;
    expect(play.disabled).toBe(true);
    fireEvent.click(speaker);
    fireEvent.click(await screen.findByRole("button", { name: /^Play$/ }));
    await waitFor(() => expect(screen.getByRole("button", { name: /Disconnect/ })).toBeTruthy(), {
      timeout: 3000,
    });
    expect(screen.getByRole("button", { name: /Pause/ })).toBeTruthy();
  });
});
