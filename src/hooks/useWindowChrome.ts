import { useEffect } from "react";
import { currentTauriWindow } from "../lib/tauri";
import type { ThemeMode } from "../types";

export function useWindowChrome(theme: ThemeMode) {
  const nativeMac = /Mac/.test(navigator.platform) && currentTauriWindow() !== null;

  useEffect(() => {
    if (!nativeMac) return;
    // Keep AppKit's edge and traffic lights in the same appearance as the page.
    void currentTauriWindow()
      ?.setTheme(theme === "system" ? null : theme)
      .catch((error) => console.warn("同步窗口外观失败:", error));
  }, [nativeMac, theme]);

  return nativeMac;
}
