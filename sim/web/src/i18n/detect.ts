import type { UiLanguage } from "./languages"

/** Tolerates browsers or embedded webviews that lack `navigator.languages` or `navigator.language`. */
export function preferredLanguages(nav?: {
  readonly languages?: readonly string[] | undefined
  readonly language?: string | undefined
}): readonly string[] {
  if (nav?.languages && nav.languages.length > 0) return nav.languages
  return nav?.language ? [nav.language] : []
}

/** The language to show: the stored choice, else the browser's first supported language. */
export function currentUiLanguage(
  stored: UiLanguage | null,
  nav: Parameters<typeof preferredLanguages>[0] = globalThis.navigator,
): UiLanguage {
  return stored ?? detectUiLanguage(preferredLanguages(nav))
}

// First-visit default only (a stored choice wins); any Chinese -> zh-TW, unsupported -> ja.
export function detectUiLanguage(preferred: readonly string[]): UiLanguage {
  for (const tag of preferred) {
    const primary = tag.trim().toLowerCase().split(/[-_]/, 1)[0]
    switch (primary) {
      case "zh":
        return "zh-TW"
      case "ja":
        return "ja"
      case "en":
        return "en"
      default:
        break
    }
  }
  return "ja"
}
