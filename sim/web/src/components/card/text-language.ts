import { useTranslation } from "react-i18next"

/** A resolved UI language as the text language the resolver understands. */
export function textLanguage(resolvedLanguage: string | undefined): string {
  return resolvedLanguage === "zh-TW" ? "zh-Hant" : (resolvedLanguage ?? "ja")
}

/** The UI language as the text language the resolver understands. */
export function useTextLanguage(): string {
  const { i18n } = useTranslation()
  return textLanguage(i18n.resolvedLanguage)
}
