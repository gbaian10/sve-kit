import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { InlineText } from "../../components/card/InlineText"
import { OriginalBox, PairedLines } from "../../components/card/OriginalBox"
import { useTextLanguage } from "../../components/card/text-language"
import { Button } from "../../components/ui/Button"
import { Link } from "../../components/ui/Link"
import {
  arrayValue,
  type CardErratum,
  type JsonValue,
  loadCardErrata,
  objectValue,
  type SnapshotClient,
  stringValue,
  textDiff,
  type TextRange,
} from "../../data"
import { usePrefs } from "../../settings"

const SIDES = ["before", "after"] as const

const wording = (value: JsonValue | undefined) =>
  Array.isArray(value)
    ? value.map((item) => stringValue(item)).join(" / ")
    : typeof value === "number"
      ? String(value)
      : typeof value === "string"
        ? value
        : ""

function Wording({
  text,
  lang,
  side,
  ranges = [],
  line,
}: {
  readonly text: string
  readonly lang: string
  readonly side?: "before" | "after"
  readonly ranges?: readonly TextRange[]
  readonly line?: number | undefined
}) {
  return (
    <p lang={lang} className="whitespace-pre-wrap">
      <InlineText
        text={text}
        emphasis
        line={line}
        changes={{ kind: side === "before" ? "removed" : "added", ranges }}
      />
    </p>
  )
}

function PairedWording({
  original,
  translation,
  region,
  side,
  ranges,
}: {
  readonly original: string
  readonly translation: string | undefined
  readonly region: string
  readonly side: "before" | "after"
  readonly ranges: readonly TextRange[]
}) {
  const prefs = usePrefs()
  const language = useTextLanguage()
  const source = (line?: number) => (
    <Wording
      text={original}
      lang={region === "jp" ? "ja" : "en"}
      side={side}
      ranges={ranges}
      line={line}
    />
  )
  if (!translation || language !== "zh-Hant" || prefs.effectLanguage === "original")
    return <OriginalBox>{source()}</OriginalBox>
  const lines = translation.split("\n").length
  // JP-source translations keep their source alongside them, as the card text does.
  const showOriginal = prefs.effectLanguage === "both" || region === "jp"
  return (
    <div className="space-y-2 text-16 leading-effect text-text-1">
      <PairedLines
        lines={lines === original.split("\n").length ? lines : 1}
        translated={(line) => <Wording text={translation} lang="zh-Hant" line={line} />}
        original={showOriginal ? source : null}
      />
    </div>
  )
}

export function CardErrata({
  client,
  cardId,
  printingId,
  faceId,
  region,
}: {
  readonly client: SnapshotClient
  readonly cardId: string
  readonly printingId: string
  readonly faceId: string
  readonly region: string
}) {
  const { t } = useTranslation()
  const language = useTextLanguage()
  const prefs = usePrefs()
  const snapshot = client.snapshot()
  const [attempt, retry] = useState(0)
  const key = JSON.stringify([snapshot?.manifestHash, cardId, printingId, faceId, region, attempt])
  const [state, setState] = useState<{
    key: string
    notices?: readonly CardErratum[]
    failed?: boolean
  }>()
  useEffect(() => {
    if (!snapshot) return
    let active = true
    void loadCardErrata(client, snapshot, cardId, printingId, faceId, region).then(
      (notices) => {
        if (active) setState({ key, notices })
      },
      () => {
        if (active) setState({ key, failed: true })
      },
    )
    return () => {
      active = false
    }
  }, [client, snapshot, cardId, printingId, faceId, region, key])
  if (state?.key !== key) return null
  if (state.failed)
    return (
      <Button
        onClick={() => {
          retry(attempt + 1)
        }}
      >
        {t("errata.retry")}
      </Button>
    )
  if (!state.notices?.length) return null
  return (
    <section
      aria-label={t("errata.heading")}
      className="mt-4 space-y-4 rounded-block border border-border bg-surface-1 px-3 py-3"
    >
      <h3 className="text-14 font-semibold text-text-1">{t("errata.heading")}</h3>
      <p className="text-12 text-text-3">{t("errata.diffHint")}</p>
      {state.notices.map(({ url, version, changes, printed, printings, fullTranslation }) => (
        <div key={stringValue(version["id"])} className="space-y-3">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-12 text-text-2">
            <Link to={url} target="_blank" rel="noreferrer">
              {t("errata.official")}
            </Link>
          </div>
          {printings.some(({ cardNo }) => cardNo) && (
            <p className="text-12 leading-relaxed text-text-3">
              {t("errata.printings")}{" "}
              {printings
                .filter(({ cardNo }) => cardNo)
                .map(({ cardNo, confirmed }) =>
                  t(confirmed ? "errata.printingConfirmed" : "errata.printingListed", { cardNo }),
                )
                .join(t("errata.listSeparator"))}
            </p>
          )}
          {printed.before && printed.after !== undefined ? (
            <div className="space-y-3 border-t border-border pt-3">
              <h4 className="text-14 font-semibold text-text-1">
                {t(
                  printed.state === "derived_from_errata"
                    ? "errata.printedHeading"
                    : "errata.reconstructedHeading",
                )}
              </h4>
              <p className="text-12 text-text-3">
                {t(
                  printed.state === "derived_from_errata"
                    ? "errata.reconstructedConfirmed"
                    : "errata.reconstructedUnknown",
                )}
              </p>
              {fullTranslation && language === "zh-Hant" && prefs.effectLanguage !== "original" && (
                <p className="text-12 text-text-3">
                  {t(
                    fullTranslation.lowConfidence
                      ? "errata.translationLowConfidence"
                      : "errata.translation",
                  )}
                </p>
              )}
              <PairedWording
                original={printed.before}
                translation={fullTranslation?.before}
                region={region}
                side="before"
                ranges={printed.diff?.removed ?? []}
              />
              <h4 className="text-12 text-text-3">{t("errata.correctedHeading")}</h4>
              <PairedWording
                original={printed.after}
                translation={fullTranslation?.after}
                region={region}
                side="after"
                ranges={printed.diff?.added ?? []}
              />
            </div>
          ) : (
            <>
              <p className="text-12 text-text-3">{t("errata.fragmentHint")}</p>
              {changes.length > 0 && (
                <p className="text-12 text-text-3">
                  {t(`errata.reason.${printed.reason ?? "source_missing"}`)}
                </p>
              )}
            </>
          )}
          {!changes.length && <p className="text-13 text-text-2">{t("errata.nonText")}</p>}
          {changes.map((change, index) => {
            const translation = arrayValue(change["wording_translations"])
              .map((value) => objectValue(value))
              .find((item) => item["lang"] === "zh-Hant")
            const showTranslation =
              translation && language === "zh-Hant" && prefs.effectLanguage !== "original"
            if (
              printed.before &&
              change["field"] === "effect" &&
              (fullTranslation || !showTranslation)
            )
              return null
            const before = wording(change["before"])
            const after = wording(change["after"])
            const diff = textDiff(before, after)
            return (
              <div key={index} className="space-y-3 border-t border-border pt-3">
                {showTranslation && (
                  <p className="text-12 text-text-3">
                    {translation["low_confidence"] === true
                      ? t("errata.translationLowConfidence")
                      : t("errata.translation")}
                  </p>
                )}
                {SIDES.map((side) => (
                  <div key={side}>
                    <h4 className="mb-1 text-12 text-text-3">{t(`errata.${side}`)}</h4>
                    <PairedWording
                      original={side === "before" ? before : after}
                      translation={showTranslation ? stringValue(translation[side]) : undefined}
                      region={region}
                      side={side}
                      ranges={side === "before" ? diff.removed : diff.added}
                    />
                  </div>
                ))}
              </div>
            )
          })}
        </div>
      ))}
    </section>
  )
}
