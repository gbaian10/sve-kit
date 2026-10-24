import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { InlineText } from "../../components/card/InlineText"
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
} from "../../data"

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
  before,
}: {
  readonly text: string
  readonly lang: string
  readonly before: boolean
}) {
  const content = <InlineText text={text} emphasis />
  return (
    <p lang={lang} className="text-16 leading-effect whitespace-pre-wrap text-text-1">
      {before ? <del>{content}</del> : content}
    </p>
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
      {state.notices.map(({ url, version, changes, printed, printings }) => (
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
              <p
                lang={region === "jp" ? "ja" : "en"}
                className="text-16 leading-effect whitespace-pre-wrap text-text-1"
              >
                {printed.before.map(({ text, changed }, index) =>
                  changed ? (
                    <del key={index}>
                      <InlineText text={text} emphasis />
                    </del>
                  ) : (
                    <InlineText key={index} text={text} emphasis />
                  ),
                )}
              </p>
              <h4 className="text-12 text-text-3">{t("errata.correctedHeading")}</h4>
              <Wording text={printed.after} lang={region === "jp" ? "ja" : "en"} before={false} />
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
            if (printed.before && change["field"] === "effect" && !translation) return null
            return (
              <div key={index} className="space-y-3 border-t border-border pt-3">
                {translation && (
                  <p className="text-12 text-text-3">
                    {translation["low_confidence"] === true
                      ? t("errata.translationLowConfidence")
                      : t("errata.translation")}
                  </p>
                )}
                {SIDES.map((side) => (
                  <div key={side}>
                    <h4 className="mb-1 text-12 text-text-3">{t(`errata.${side}`)}</h4>
                    <div className="space-y-2">
                      {translation && (
                        <Wording
                          text={stringValue(translation[side])}
                          lang="zh-Hant"
                          before={side === "before"}
                        />
                      )}
                      {!(printed.before && change["field"] === "effect") && (
                        <div className="rounded-control border border-border px-3 py-2">
                          <p className="mb-1 text-12 text-text-3">{t("card.originalText")}</p>
                          <Wording
                            text={wording(change[side])}
                            lang={region === "jp" ? "ja" : "en"}
                            before={side === "before"}
                          />
                        </div>
                      )}
                    </div>
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
