import type { EngineState } from "../lib/types";
import { cn } from "./ui";

export function describeState(state: EngineState): { label: string; tone: string; pulse: boolean } {
  switch (state.kind) {
    case "idle":
      return { label: "Not connected", tone: "bg-border", pulse: false };
    case "connecting":
      return { label: "Connecting…", tone: "bg-warn", pulse: true };
    case "streaming":
      return { label: "Streaming", tone: "bg-ok", pulse: false };
    case "paused":
      return { label: "Paused", tone: "bg-warn", pulse: false };
    case "reconnecting":
      return { label: `Reconnecting (try ${state.attempt})…`, tone: "bg-warn", pulse: true };
    case "error":
      return { label: "Error", tone: "bg-danger", pulse: false };
  }
}

export function StatusPill({ state }: { state: EngineState }) {
  const { label, tone, pulse } = describeState(state);
  return (
    <span
      role="status"
      className="inline-flex items-center gap-2 rounded-full bg-surface-2 px-2.5 py-1 text-xs font-medium"
    >
      <span className={cn("h-2 w-2 rounded-full", tone, pulse && "animate-pulse")} />
      {label}
    </span>
  );
}
