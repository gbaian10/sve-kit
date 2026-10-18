import { useTranslation } from "react-i18next"

import { useAnnotatedText } from "../../app/snapshot"
import type { AnnotatedTextResolver, JsonObject, SnapshotClient } from "../../data"
import type { DisplayedName } from "../../domain/nameDisplay"
import type { Region } from "../../domain/search"
import { usePrefs } from "../../settings"
import { AnnotatedText } from "./AnnotatedText"
import { useTextLanguage } from "./text-language"
import { TextPurposeStatus } from "./TextPurposeStatus"
import { TranslationSourceLine } from "./TranslationSourceLine"

const HEADING = "text-20 leading-tight font-bold wrap-anywhere"

/** The catalog's name: shown while the exact owner resolves, and for faces without a revision. */
export function PlainNameHeading({ name }: { readonly name: DisplayedName }) {
  const { t } = useTranslation()
  return (
    <>
      <h2 lang={name.primary.lang} className={HEADING}>
        {name.primary.text}
      </h2>
      {name.secondary && (
        <p lang={name.secondary.lang} className="text-14 text-text-2">
          {name.secondary.text}
        </p>
      )}
      {name.missingTranslation && <p className="text-12 text-text-3">{t("card.noTranslation")}</p>}
      {(name.lowConfidence || name.jpSource) && (
        <p className="flex flex-wrap gap-x-2 text-12 text-text-3">
          {name.lowConfidence && <span>{t("card.translationProofreading")}</span>}
          {name.jpSource && <span>{t("card.jpSource")}</span>}
        </p>
      )}
    </>
  )
}

// The exact owner keeps whole-name explanations and translation authority attached to the name.
export function CardNameHeading({
  client,
  resolver,
  owner,
  edition,
  fallback,
}: {
  readonly client: SnapshotClient
  readonly resolver: AnnotatedTextResolver | null
  readonly owner: JsonObject
  readonly edition: Region
  readonly fallback: DisplayedName
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const language = useTextLanguage()
  const { readiness, value } = useAnnotatedText(client, owner, "name", language, edition, resolver)
  if (readiness.basic !== "ready" || !value)
    return (
      <>
        <PlainNameHeading name={fallback} />
        <TextPurposeStatus readiness={readiness} />
      </>
    )
  const mode = prefs.nameDisplay
  const translated = mode !== "original" ? value.translated : undefined
  const primary = mode === "both" ? value.original : (translated ?? value.original)
  const secondary =
    translated &&
    (mode === "both"
      ? translated
      : value.translation?.["low_confidence"] === true
        ? value.original
        : undefined)
  const annotationReady = readiness.annotations === "ready" || readiness.annotations === "none"
  const emphasis = prefs.termEmphasis
  return (
    <>
      <AnnotatedText
        value={primary}
        context={value}
        emphasis={emphasis}
        annotationReady={annotationReady}
        client={client}
        edition={edition}
        heading
        className={HEADING}
      />
      {secondary && (
        <AnnotatedText
          value={secondary}
          context={value}
          emphasis={emphasis}
          annotationReady={annotationReady}
          client={client}
          edition={edition}
          className="text-14 text-text-2"
        />
      )}
      <TextPurposeStatus readiness={readiness} />
      {translated && <TranslationSourceLine value={value} />}
      {translated && value.selection?.["basis"] === "jp_source" && (
        <p className="text-12 text-text-3">{t("card.jpSource")}</p>
      )}
      {mode !== "original" && !value.translated && value.original.unit["lang"] !== language && (
        <p className="text-12 text-text-3">{t("card.noTranslation")}</p>
      )}
    </>
  )
}
