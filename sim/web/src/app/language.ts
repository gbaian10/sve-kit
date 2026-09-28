import { useEffect } from "react"
import { useTranslation } from "react-i18next"

import { currentUiLanguage } from "../i18n"
import { usePrefs } from "../settings"

export { currentUiLanguage }

// Prefs are the source of truth; i18next and <html lang> follow them.
export function useUiLanguageSync(): void {
  const { uiLanguage } = usePrefs()
  const { i18n } = useTranslation()
  const language = currentUiLanguage(uiLanguage)
  useEffect(() => {
    document.documentElement.lang = language
    if (i18n.resolvedLanguage !== language) void i18n.changeLanguage(language)
  }, [i18n, language])
}
