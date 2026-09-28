import { createInstance, type i18n } from "i18next"
import { initReactI18next } from "react-i18next"

import { UI_LANGUAGES, type UiLanguage } from "./languages"
import { pseudoPostProcessor } from "./pseudo"
import { defaultNS, resources } from "./resources"

export { currentUiLanguage } from "./detect"
export { UI_LANGUAGES, type UiLanguage } from "./languages"

export interface I18nOptions {
  /** Dev only: run every string through the pseudo-locale to stress the layout. */
  readonly pseudo?: boolean
}

export async function createI18n(
  lng: UiLanguage,
  { pseudo = false }: I18nOptions = {},
): Promise<i18n> {
  const instance = createInstance()
  instance.use(initReactI18next)
  if (pseudo) instance.use(pseudoPostProcessor)
  await instance.init({
    lng,
    resources,
    defaultNS,
    supportedLngs: UI_LANGUAGES,
    // Same fallback as unsupported browser languages (see detectUiLanguage).
    fallbackLng: "ja",
    // React already escapes interpolated values.
    interpolation: { escapeValue: false },
    ...(pseudo ? { postProcess: "pseudo" } : {}),
  })
  return instance
}
