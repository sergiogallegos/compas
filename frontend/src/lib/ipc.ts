// Typed wrappers over Tauri commands. Importing `invoke` lazily means the dev
// server can run in a plain browser (outside Tauri) without crashing — calls just
// reject, which the UI can surface.

import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  version: string;
  phase: string;
}

/** True when running inside the Tauri webview (vs. a plain browser dev tab). */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function appInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

export async function setCrossfader(value: number): Promise<void> {
  await invoke("set_crossfader", { value });
}

export async function setMasterGain(value: number): Promise<void> {
  await invoke("set_master_gain", { value });
}

export async function setDeckGain(deck: number, gain: number): Promise<void> {
  await invoke("set_deck_gain", { deck, gain });
}
