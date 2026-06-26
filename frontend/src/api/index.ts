import { browserMockApi } from "./browserMock";
import { tauriApi } from "./tauriClient";

const canUseTauri =
  typeof window !== "undefined" &&
  typeof (window as Window & { __TAURI_IPC__?: unknown }).__TAURI_IPC__ === "function";
const useBrowserMock = !canUseTauri;

export type SkillHubApi = typeof tauriApi;
export const api: SkillHubApi = useBrowserMock ? browserMockApi : tauriApi;
