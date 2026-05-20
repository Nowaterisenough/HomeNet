import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

type CurrentWindow = ReturnType<typeof getCurrentWindow>;

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function invokeCommand<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauriRuntime()) {
    throw new Error(`Tauri runtime unavailable for command: ${command}`);
  }
  return invoke<T>(command, args);
}

export function currentTauriWindow(): CurrentWindow | null {
  if (!isTauriRuntime()) return null;
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

export async function minimizeCurrentWindow(): Promise<void> {
  await currentTauriWindow()?.minimize();
}

export async function toggleMaximizeCurrentWindow(): Promise<void> {
  const appWindow = currentTauriWindow();
  if (!appWindow) return;
  if (await appWindow.isMaximized()) {
    await appWindow.unmaximize();
  } else {
    await appWindow.maximize();
  }
}

export async function closeCurrentWindow(): Promise<void> {
  await currentTauriWindow()?.close();
}

export async function startCurrentWindowDrag(): Promise<void> {
  await currentTauriWindow()?.startDragging();
}
