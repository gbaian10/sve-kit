import type { UiLanguage } from "./languages"

/**
 * First-visit default only; a language the user picked is stored in settings and wins.
 * Any Chinese (including Simplified and bare `zh`) maps to Traditional Chinese, and
 * unsupported languages fall back to Japanese.
 */
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
