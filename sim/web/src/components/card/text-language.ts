import { useTranslation } from "react-i18next"

/** The UI language as the text language the resolver understands. */
export function useTextLanguage(): string {
  const { i18n } = useTranslation()
  return i18n.resolvedLanguage === "zh-TW" ? "zh-Hant" : (i18n.resolvedLanguage ?? "ja")
}
