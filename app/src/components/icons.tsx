// MingCute icons (Apache-2.0), compiled into the bundle by unplugin-icons.
import AddLine from "~icons/mingcute/add-line";
import AirplayFill from "~icons/mingcute/airplay-fill";
import AppstoreLine from "~icons/mingcute/appstore-line";
import CloseLine from "~icons/mingcute/close-line";
import ComputerLine from "~icons/mingcute/computer-line";
import DownLine from "~icons/mingcute/down-line";
import FolderOpenLine from "~icons/mingcute/folder-open-line";
import HomepodFill from "~icons/mingcute/homepod-fill";
import HomepodMiniFill from "~icons/mingcute/homepod-mini-fill";
import ImacFill from "~icons/mingcute/imac-fill";
import LaptopFill from "~icons/mingcute/laptop-fill";
import LoadingLine from "~icons/mingcute/loading-line";
import PauseFill from "~icons/mingcute/pause-fill";
import PlayFill from "~icons/mingcute/play-fill";
import PowerLine from "~icons/mingcute/power-line";
import RefreshLine from "~icons/mingcute/refresh-2-line";
import Settings3Line from "~icons/mingcute/settings-3-line";
import ShieldLine from "~icons/mingcute/shield-line";
import Tv2Fill from "~icons/mingcute/tv-2-fill";
import UpLine from "~icons/mingcute/up-line";
import VolumeFill from "~icons/mingcute/volume-fill";
import VolumeOffFill from "~icons/mingcute/volume-off-fill";
import WarningLine from "~icons/mingcute/warning-line";
import type { Speaker } from "../lib/types";

export {
  AddLine,
  AirplayFill,
  AppstoreLine,
  CloseLine,
  ComputerLine,
  DownLine,
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
};

type DeviceFamily = "homepodMini" | "homepod" | "appleTv" | "laptop" | "desktop" | "other";

/** Maps an Apple model identifier to the icon family to draw. */
export function deviceFamily(model: string): DeviceFamily {
  if (model === "AudioAccessory5,1") return "homepodMini";
  if (model.startsWith("AudioAccessory")) return "homepod";
  if (model.startsWith("AppleTV")) return "appleTv";
  if (model.startsWith("MacBook")) return "laptop";
  if (model.startsWith("iMac") || model.startsWith("Mac")) return "desktop";
  return "other";
}

const familyIcon = {
  homepodMini: HomepodMiniFill,
  homepod: HomepodFill,
  appleTv: Tv2Fill,
  laptop: LaptopFill,
  desktop: ImacFill,
  other: AirplayFill,
} as const;

/** The speaker's icon; a stereo pair shows two units side by side. */
export function DeviceIcon({ speaker, size = 22 }: { speaker: Speaker; size?: number }) {
  const family = deviceFamily(speaker.model);
  const Icon = familyIcon[family];
  if (speaker.kind === "stereoPair") {
    return (
      <span aria-hidden className="inline-flex items-center" style={{ gap: size * 0.08 }}>
        <Icon width={size * 0.82} height={size * 0.82} />
        <Icon width={size * 0.82} height={size * 0.82} />
      </span>
    );
  }
  return <Icon aria-hidden width={size} height={size} />;
}
