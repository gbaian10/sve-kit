import type { UiLanguage } from "../i18n/languages"
import { UI_TEXT_LANG } from "./nameDisplay"
import type { Region, TextLang } from "./search"

// The card-face language matrix (snapshot-format §5): the edition (region) decides the original
// text and image, the UI language decides the translation row. Japanese and English UIs never
// fall back to Traditional Chinese; a missing translation shows the original with a notice.
export type TranslationOrigin =
  "official_sve" | "official_svwb" | "official_sv1" | "project" | "machine" | "community"

export interface OriginalText {
  readonly lang: TextLang
  readonly text: string
}

export interface TranslationCandidate {
  readonly lang: TextLang
  readonly text: string
  readonly origin: TranslationOrigin
  readonly status: "draft" | "reviewed" | "stale"
  /** The FieldTranslation basis: `official_counterpart` names the other region's official text. */
  readonly basis: "own_source" | "shared_jp" | "official_counterpart"
}

export interface FaceTextInput {
  readonly edition: Region
  readonly uiLanguage: UiLanguage
  /** Original text of the edition's region, undefined when the card is not released there. */
  readonly original: OriginalText | undefined
  /** Original of the other region, used when the edition has none. */
  readonly fallback: OriginalText | undefined
  /** Translations of whichever original is shown. */
  readonly translations: readonly TranslationCandidate[]
}

export type TranslationLabel = "official" | "project" | "machine" | "community"

export interface ResolvedFaceText {
  readonly original: OriginalText
  readonly translation: (OriginalText & { readonly label: TranslationLabel }) | null
  /** `missing`: the UI language wanted a translation the snapshot lacks; `no_edition`: the edition is not released, showing the other one. */
  readonly notices: readonly ("missing" | "no_edition")[]
}

const LABEL: Record<TranslationOrigin, TranslationLabel> = {
  official_sve: "official",
  official_svwb: "official",
  official_sv1: "official",
  project: "project",
  machine: "machine",
  community: "community",
}
const PRIORITY: readonly TranslationOrigin[] = [
  "official_sve",
  "official_svwb",
  "official_sv1",
  "project",
  "community",
  "machine",
]

function pick(
  candidates: readonly TranslationCandidate[],
  lang: TextLang,
): TranslationCandidate | undefined {
  const usable = candidates.filter((item) => item.lang === lang && item.text !== "")
  // A reviewed text beats a draft; among equals the official one wins.
  return [...usable].sort(
    (a, b) =>
      Number(a.status === "draft") - Number(b.status === "draft") ||
      PRIORITY.indexOf(a.origin) - PRIORITY.indexOf(b.origin),
  )[0]
}

export function resolveFaceText(input: FaceTextInput): ResolvedFaceText | null {
  const original = input.original ?? input.fallback
  if (!original) return null
  const notices: ("missing" | "no_edition")[] = []
  if (!input.original) notices.push("no_edition")
  const wanted = UI_TEXT_LANG[input.uiLanguage]
  if (wanted === original.lang) return { original, translation: null, notices }
  const translation = pick(input.translations, wanted)
  if (!translation) {
    notices.push("missing")
    return { original, translation: null, notices }
  }
  return {
    original,
    translation: { lang: wanted, text: translation.text, label: LABEL[translation.origin] },
    notices,
  }
}
