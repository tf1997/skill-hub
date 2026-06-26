import { useCallback, useEffect, useState } from "react";

export type ThemeName = "light" | "dark";

const THEME_STORAGE_KEY = "skill-hub-theme";

function initialTheme(): ThemeName {
  if (typeof document !== "undefined" && document.documentElement.dataset.theme === "dark") {
    return "dark";
  }
  if (typeof window !== "undefined") {
    let stored: string | null = null;
    try {
      stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    } catch {
      stored = null;
    }
    if (stored === "dark" || stored === "light") {
      return stored;
    }
    if (window.matchMedia?.("(prefers-color-scheme: dark)").matches) {
      return "dark";
    }
  }
  return "light";
}

export function useTheme() {
  const [theme, setTheme] = useState<ThemeName>(() => initialTheme());

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, theme);
    } catch {
      // Storage can be unavailable in constrained WebView environments.
    }
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme((current) => (current === "light" ? "dark" : "light"));
  }, []);

  return {
    theme,
    setTheme,
    toggleTheme
  };
}
