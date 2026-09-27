import type { UiLanguage } from "./languages"
import en from "./locales/en"
import ja from "./locales/ja"
import zhTW, { type Messages } from "./locales/zh-TW"

export const defaultNS = "translation"

export const resources = {
  "zh-TW": { translation: zhTW },
  ja: { translation: ja },
  en: { translation: en },
} as const satisfies Record<UiLanguage, { translation: Messages }>
