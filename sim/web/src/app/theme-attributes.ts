import { useEffect } from "react"

import type { ThemeChoice } from "../domain/theme"
import { usePrefs } from "../settings"

// `system` and a null accent leave the attributes off so tokens.css falls back to the media query
// and the per-theme default accent. Same behaviour as public/theme-boot.js.
export function applyThemeAttributes(
  choice: ThemeChoice,
  root: HTMLElement = document.documentElement,
): void {
  if (choice.theme === "system") root.removeAttribute("data-theme")
  else root.setAttribute("data-theme", choice.theme)
  if (choice.accent === null) root.removeAttribute("data-accent")
  else root.setAttribute("data-accent", choice.accent)
}

export function useThemeAttributes(): void {
  const { theme, accent } = usePrefs()
  useEffect(() => {
    applyThemeAttributes({ theme, accent })
  }, [theme, accent])
}
