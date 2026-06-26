import { Moon, Sun } from "lucide-react";

export function ThemeSwitch(props: {
  theme: "light" | "dark";
  onTheme: (theme: "light" | "dark") => void;
}) {
  const isLight = props.theme === "light";
  const Icon = isLight ? Sun : Moon;
  const nextTheme = isLight ? "dark" : "light";

  return (
    <button
      className="sidebar-theme-switch"
      onClick={() => props.onTheme(nextTheme)}
      title={isLight ? "切换到深色" : "切换到白色"}
      aria-label={isLight ? "切换到深色" : "切换到白色"}
      type="button"
    >
      <Icon size={17} />
    </button>
  );
}
