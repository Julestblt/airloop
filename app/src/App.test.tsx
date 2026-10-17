import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import App, { isActive } from "./App";
import { describeState } from "./components/StatusPill";
import { SpeakerList } from "./components/SpeakerList";
import { idleStatus } from "./lib/api";
import type { Speaker } from "./lib/types";

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

describe("state helpers", () => {
  it("labels every state", () => {
    expect(describeState({ kind: "idle" }).label).toBe("Not connected");
    expect(describeState({ kind: "reconnecting", attempt: 2 }).label).toContain("2");
    expect(describeState({ kind: "error" }).tone).toBe("bg-danger");
  });

  it("treats connecting/streaming/paused/reconnecting as active", () => {
    expect(isActive(idleStatus)).toBe(false);
    for (const kind of ["connecting", "streaming", "paused"] as const) {
      expect(isActive({ ...idleStatus, state: { kind } })).toBe(true);
    }
    expect(isActive({ ...idleStatus, state: { kind: "error" } })).toBe(false);
  });
});

describe("SpeakerList", () => {
  it("shows a searching state when empty", () => {
    render(
      <SpeakerList
        speakers={[]}
        selected={[]}
        onToggle={() => {}}
        locked={false}
        connectedIds={[]}
      />,
    );
    expect(screen.getByText(/Looking for speakers/)).toBeTruthy();
  });

  it("renders a stereo pair as one selectable entry", () => {
    const toggled: string[] = [];
    render(
      <SpeakerList
        speakers={[pair]}
        selected={[]}
        onToggle={(id) => toggled.push(id)}
        locked={false}
        connectedIds={[]}
      />,
    );
    expect(screen.getByText("HomePod mini stereo pair")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Living Room/ }));
    expect(toggled).toEqual(["pair-1"]);
  });

  it("disables unsupported speakers", () => {
    render(
      <SpeakerList
        speakers={[{ ...pair, unsupportedReason: "Needs a password" }]}
        selected={[]}
        onToggle={() => {}}
        locked={false}
        connectedIds={[]}
      />,
    );
    const button = screen.getByRole("button", { name: /Living Room/ }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });
});

describe("App (mock backend)", () => {
  it("connects to a selected speaker and shows streaming", async () => {
    render(<App />);
    const speaker = await screen.findByRole("button", { name: /Living Room/ });
    const connect = screen.getByRole("button", { name: /Select a speaker/ }) as HTMLButtonElement;
    expect(connect.disabled).toBe(true);
    fireEvent.click(speaker);
    fireEvent.click(await screen.findByRole("button", { name: /^Connect$/ }));
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Streaming"), {
      timeout: 3000,
    });
    expect(screen.getByRole("button", { name: /Disconnect/ })).toBeTruthy();
  });
});
