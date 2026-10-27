import type { UiLanguage } from "./languages"

/** First-visit default only (a stored choice wins); any Chinese -> zh-TW, unsupported -> ja.
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
