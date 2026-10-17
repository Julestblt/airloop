import { Check, Loader2, Speaker as SpeakerIcon } from "lucide-react";
import type { Speaker } from "../lib/types";
import { cn } from "./ui";

function PairIcon() {
  return (
    <span className="relative inline-flex">
      <SpeakerIcon className="h-5 w-5" />
      <SpeakerIcon className="-ml-2 h-5 w-5 opacity-60" />
    </span>
  );
}

export function SpeakerList({
  speakers,
  selected,
  onToggle,
  locked,
  connectedIds,
}: {
  speakers: Speaker[];
  selected: string[];
  onToggle: (id: string) => void;
  /** True while a session is active: selection cannot change. */
  locked: boolean;
  connectedIds: string[];
}) {
  if (speakers.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 rounded-[var(--radius-card)] border border-dashed border-border px-6 py-10 text-center">
        <Loader2 className="h-6 w-6 animate-spin text-muted" />
        <div>
          <p className="font-medium">Looking for speakers…</p>
          <p className="mt-1 text-xs text-muted">
            Make sure your HomePod is powered on and on the same network as this PC.
          </p>
        </div>
      </div>
    );
  }

  return (
    <ul className="flex flex-col gap-2" aria-label="Speakers">
      {speakers.map((s) => {
        const isSelected = selected.includes(s.id);
        const isConnected = connectedIds.includes(s.id);
        const disabled = locked || s.unsupportedReason !== null;
        return (
          <li key={s.id}>
            <button
              onClick={() => onToggle(s.id)}
              disabled={disabled}
              aria-pressed={isSelected}
              title={s.unsupportedReason ?? undefined}
              className={cn(
                "flex w-full items-center gap-3 rounded-[var(--radius-card)] border px-4 py-3 text-left transition",
                "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                isSelected
                  ? "border-accent/60 bg-accent/10"
                  : "border-border bg-surface hover:bg-surface-2",
                disabled && !isSelected && "opacity-50",
                locked && "cursor-default",
              )}
            >
              <span
                className={cn(
                  "flex h-9 w-9 items-center justify-center rounded-full",
                  isSelected ? "bg-accent text-accent-fg" : "bg-surface-2 text-muted",
                )}
              >
                {s.kind === "stereoPair" ? <PairIcon /> : <SpeakerIcon className="h-5 w-5" />}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{s.name}</span>
                <span className="block truncate text-xs text-muted">
                  {s.unsupportedReason ?? s.modelName}
                </span>
              </span>
              {isConnected ? (
                <span className="flex items-center gap-1.5 text-xs font-medium text-ok">
                  <span className="h-2 w-2 rounded-full bg-ok" /> Playing
                </span>
              ) : (
                <span
                  className={cn(
                    "flex h-5 w-5 items-center justify-center rounded-md border",
                    isSelected ? "border-accent bg-accent text-accent-fg" : "border-border",
                  )}
                >
                  {isSelected && <Check className="h-3.5 w-3.5" strokeWidth={3} />}
                </span>
              )}
            </button>
          </li>
        );
      })}
    </ul>
  );
}
