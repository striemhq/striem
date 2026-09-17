"use client";

import { useEffect, useState } from "react";

type Theme = "light" | "dark" | "glitter";

const THEMES: { id: Theme; label: string; icon: string }[] = [
  { id: "light", label: "Light", icon: "☀️" },
  { id: "dark", label: "Dark", icon: "🌙" },
  { id: "glitter", label: "Glitter", icon: "✨" },
];

// A three-way theme switch. The theme is set on <html data-theme> (the script in
// layout.tsx sets it before paint) and kept in localStorage.
export default function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>("light");

  useEffect(() => {
    const current = document.documentElement.dataset.theme as Theme | undefined;
    if (current) setTheme(current);
  }, []);

  const choose = (t: Theme) => {
    document.documentElement.dataset.theme = t;
    try {
      localStorage.setItem("striem-theme", t);
    } catch {
      /* ignore storage errors */
    }
    setTheme(t);
  };

  return (
    <div className="theme-toggle">
      {THEMES.map((t) => (
        <button
          key={t.id}
          type="button"
          onClick={() => choose(t.id)}
          className={`theme-toggle-btn ${theme === t.id ? "theme-toggle-active" : ""}`}
          title={`${t.label} mode`}
          aria-label={`${t.label} mode`}
          aria-pressed={theme === t.id}
        >
          <span aria-hidden>{t.icon}</span>
        </button>
      ))}
    </div>
  );
}
