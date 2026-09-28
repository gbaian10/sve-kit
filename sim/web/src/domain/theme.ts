export const THEME_PREFS = ["system", "light", "dark"] as const
export type ThemePref = (typeof THEME_PREFS)[number]

export const ACCENTS = ["amber", "teal", "red"] as const
export type Accent = (typeof ACCENTS)[number]

export type ResolvedTheme = "light" | "dark"

export interface ThemeChoice {
  readonly theme: ThemePref
  readonly accent: Accent | null
}

export interface ResolvedThemeChoice {
  readonly theme: ResolvedTheme
  readonly accent: Accent
}

export function isThemePref(value: unknown): value is ThemePref {
  return typeof value === "string" && (THEME_PREFS as readonly string[]).includes(value)
}

export function isAccent(value: unknown): value is Accent {
  return typeof value === "string" && (ACCENTS as readonly string[]).includes(value)
}

// The design pairs each theme with its own default; a chosen accent applies to both themes.
export function defaultAccent(theme: ResolvedTheme): Accent {
  return theme === "dark" ? "amber" : "teal"
}

// Same rule as public/theme-boot.js; the boot file cannot import this, so a test compares both.
export function resolveTheme(choice: ThemeChoice, systemDark: boolean): ResolvedThemeChoice {
  const theme: ResolvedTheme =
    choice.theme === "system" ? (systemDark ? "dark" : "light") : choice.theme
  return { theme, accent: choice.accent ?? defaultAccent(theme) }
}
