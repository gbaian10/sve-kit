import type { UiLanguage } from "../i18n/languages"
import type { TextLang } from "./search"

// Card names on lists (design decision 20): the edition picks the original, the UI language picks
// the translation row, and the name-display preference decides which one leads. Japanese and English
// UIs never fall back to Traditional Chinese (snapshot-format §5).
export type NameDisplay = "translated" | "original" | "both"

export interface NameSource {
  readonly original: { readonly lang: TextLang; readonly text: string }
  readonly translations: Partial<Record<TextLang, string>>
  readonly translationQuality?: Partial<
    Record<
      TextLang,
      {
        readonly lowConfidence: boolean
        readonly sourceUnchecked: boolean
      }
    >
  >
}

export interface DisplayedText {
  readonly text: string
  readonly lang: TextLang
}

export interface DisplayedName {
  readonly primary: DisplayedText
  readonly secondary?: DisplayedText
  /** True when the preference asked for a translation the snapshot does not have. */
  readonly missingTranslation: boolean
  readonly lowConfidence?: boolean
  readonly sourceUnchecked?: boolean
}

export const UI_TEXT_LANG: Record<UiLanguage, TextLang> = { "zh-TW": "zh-Hant", ja: "ja", en: "en" }

export function displayName(
  source: NameSource,
  uiLanguage: UiLanguage,
  display: NameDisplay,
): DisplayedName {
  const lang = UI_TEXT_LANG[uiLanguage]
  const original = source.original
  const translated = source.translations[lang]
  const translation: DisplayedText | undefined =
    translated === undefined || translated === "" || lang === original.lang
      ? undefined
      : { text: translated, lang }
  if (display === "original" || lang === original.lang)
    return { primary: original, missingTranslation: false }
  if (translation === undefined) return { primary: original, missingTranslation: true }
  const quality = source.translationQuality?.[lang]
  const notices = {
    ...(quality?.lowConfidence ? { lowConfidence: true } : {}),
    ...(quality?.sourceUnchecked ? { sourceUnchecked: true } : {}),
  }
  if (display === "translated")
    return {
      primary: translation,
      ...(quality?.lowConfidence ? { secondary: original } : {}),
      missingTranslation: false,
      ...notices,
    }
  return { primary: original, secondary: translation, missingTranslation: false, ...notices }
}
