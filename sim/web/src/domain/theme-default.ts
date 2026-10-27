import type { Accent, ResolvedTheme } from "./theme"

// The design pairs each theme with its own default; a chosen accent applies to both themes.
export function defaultAccent(theme: ResolvedTheme): Accent {
  return theme === "dark" ? "amber" : "teal"
}
