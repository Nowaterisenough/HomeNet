import { useEffect, useState } from "react";
import type { ThemeMode } from "../types";

function readTheme(): ThemeMode {
  try {
    const saved = localStorage.getItem("homenet:theme");
    if (saved === "light" || saved === "dark" || saved === "system")
      return saved;
  } catch {
    // The theme still works when storage is unavailable.
  }
  return "dark";
}

export function useTheme() {
  const [theme, setTheme] = useState<ThemeMode>(readTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem("homenet:theme", theme);
    } catch {
      // Apply the current selection without persisting it.
    }
  }, [theme]);

  useEffect(() => {
    function syncTheme(event: StorageEvent) {
      if (event.key === "homenet:theme") setTheme(readTheme());
    }
    window.addEventListener("storage", syncTheme);
    return () => window.removeEventListener("storage", syncTheme);
  }, []);

  return [theme, setTheme] as const;
}
