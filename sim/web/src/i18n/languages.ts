export const UI_LANGUAGES = ["zh-TW", "ja", "en"] as const

export type UiLanguage = (typeof UI_LANGUAGES)[number]

export function isUiLanguage(value: unknown): value is UiLanguage {
  return typeof value === "string" && (UI_LANGUAGES as readonly string[]).includes(value)
}
