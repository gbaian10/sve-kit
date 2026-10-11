import { useTranslation } from "react-i18next"

import { useAnnotatedText } from "../../app/snapshot"
import {
  type AnnotatedTextResolver,
  type JsonObject,
  type SnapshotClient,
  stringValue,
} from "../../data"
import type { Region } from "../../domain/search"
import { usePrefs } from "../../settings"
import { AnnotatedText } from "./AnnotatedText"
import { useTextLanguage } from "./text-language"
import { TextPurposeStatus } from "./TextPurposeStatus"
import { TranslationSourceLine } from "./TranslationSourceLine"

// Effect text is the reason the page exists: 16px at line-height 1.75, full width (design 06).
const BODY = "text-16 leading-effect"

export function CardText({
  client,
  owner,
  field,
  edition,
  sharedResolver,
}: {
  readonly sharedResolver?: AnnotatedTextResolver | null
  readonly client: SnapshotClient
  readonly edition: Region
  readonly owner: JsonObject
  readonly field: "name" | "effect" | "flavor"
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const language = useTextLanguage()
  const { readiness, value } = useAnnotatedText(
    client,
    owner,
    field,
    language,
    edition,
    sharedResolver,
  )
  if (readiness.basic === "pending") return <p role="status">{t("card.annotationPending")}</p>
  if (readiness.basic === "error")
    return (
      <p role="alert">
        {readiness.error?.kind === "validation" ? t("card.textFailed") : t("card.textLoadFailed")}
      </p>
    )
  if (!value) return <p>{t("card.textUnavailable")}</p>
  // Leaders and some tokens have no effect text at all: say so instead of an empty section.
  if (field === "effect" && stringValue(value.original.unit["text"]) === "" && !value.translated)
    return (
      <p className="mt-3 rounded-control border border-dashed border-border-strong px-3 py-3 text-center text-14 text-text-2">
        {t("card.noEffect")}
      </p>
    )
  const annotationReady = readiness.annotations === "ready" || readiness.annotations === "none"
  const mode = field === "name" ? prefs.nameDisplay : prefs.effectLanguage
  const showOriginal =
    mode !== "translated" || !value.translated || value.selection?.["basis"] === "jp_source"
  const showTranslated = mode !== "original" && value.translated !== undefined
  return (
    <section className="mt-3 space-y-2" aria-label={t(`card.field.${field}`)} data-field={field}>
      <h3 className="text-13 font-semibold text-text-3">{t(`card.field.${field}`)}</h3>
      {value.printedState === "derived" && <p className="text-13">{t("card.printedDerived")}</p>}
      {value.printedState === "unknown" && (
        <p className="text-13">{t("card.printedUnconfirmed")}</p>
      )}
      <TextPurposeStatus readiness={readiness} />
      {showOriginal && (
        <div className={BODY}>
          <AnnotatedText
            value={value.original}
            context={value}
            emphasis={prefs.termEmphasis}
            annotationReady={annotationReady}
          />
        </div>
      )}
      {showTranslated && (
        <>
          <div className={BODY}>
            <AnnotatedText
              value={value.translated}
              context={value}
              emphasis={prefs.termEmphasis}
              annotationReady={annotationReady}
            />
          </div>
          <TranslationSourceLine value={value} />
          {value.selection?.["basis"] === "jp_source" &&
            readiness.source === "ready" &&
            value.source && (
              <details className="text-15">
                <summary className="cursor-pointer text-13 text-text-2">
                  {t("card.jpSource")}
                </summary>
                <AnnotatedText
                  value={value.source}
                  context={value}
                  emphasis={prefs.termEmphasis}
                  annotationReady={annotationReady}
                />
              </details>
            )}
        </>
      )}
      {mode !== "original" &&
        !value.translated &&
        stringValue(value.original.unit["lang"]) !== language && (
          <p className="text-13 text-text-2">{t("card.translationMissing")}</p>
        )}
      <button
        type="button"
        className="h-9 rounded-control px-2 text-13 font-semibold text-accent-text hover:bg-surface-2"
        onClick={() => {
          void navigator.clipboard.writeText(
            stringValue(
              (mode !== "original" && value.translated ? value.translated : value.original).unit[
                "text"
              ],
            ),
          )
        }}
      >
        {t("card.copyText")}
      </button>
    </section>
  )
}
