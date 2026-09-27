import { createInstance, type i18n } from "i18next"
import { initReactI18next } from "react-i18next"

import { UI_LANGUAGES, type UiLanguage } from "./languages"
import { defaultNS, resources } from "./resources"

export { detectUiLanguage, preferredLanguages } from "./detect"
export { isUiLanguage, UI_LANGUAGES, type UiLanguage } from "./languages"

export async function createI18n(lng: UiLanguage): Promise<i18n> {
  const instance = createInstance()
  await instance.use(initReactI18next).init({
    lng,
    resources,
    defaultNS,
    supportedLngs: UI_LANGUAGES,
    // Same fallback as unsupported browser languages (see detectUiLanguage).
    fallbackLng: "ja",
    // React already escapes interpolated values.
    interpolation: { escapeValue: false },
  })
  return instance
}
