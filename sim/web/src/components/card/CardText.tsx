import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import {
  canonicalText,
  createAnnotatedTextResolver,
  type JsonObject,
  type SelectedText,
  type SnapshotClient,
  stringValue,
} from "../../data"
import { usePrefs } from "../../settings"
import { AnnotatedText } from "./AnnotatedText"

export function CardText({
  client,
  owner,
  field,
}: {
  readonly client: SnapshotClient
  readonly owner: JsonObject
  readonly field: "name" | "effect" | "flavor"
}) {
  const { t, i18n } = useTranslation()
  const prefs = usePrefs()
  const snapshot = client.snapshot()
  const language = i18n.resolvedLanguage === "zh-TW" ? "zh-Hant" : (i18n.resolvedLanguage ?? "ja")
  const identity = canonicalText([owner, field, language])
  const resolver = useMemo(
    () => (snapshot ? createAnnotatedTextResolver(client) : null),
    [client, snapshot],
  )
  const [loaded, setLoaded] = useState<{
    readonly snapshot: typeof snapshot
    readonly identity: string
    readonly value?: SelectedText | null
    readonly failed?: boolean
  }>()
  useEffect(() => {
    if (!resolver) return
    let active = true
    const pointer = { owner, field, ordinal: null }
    void resolver.resolve(pointer, language).then(
      (value) => {
        if (active) setLoaded({ snapshot, identity, value })
      },
      () => {
        if (active) setLoaded({ snapshot, identity, failed: true })
      },
    )
    return () => {
      active = false
    }
  }, [resolver, snapshot, identity, owner, field, language])
  if (loaded?.snapshot !== snapshot || loaded.identity !== identity)
    return <p role="status">{t("card.annotationPending")}</p>
  if (loaded.failed) return <p role="alert">{t("card.textFailed")}</p>
  const value = loaded.value
  if (!value) return <p>{t("card.textUnavailable")}</p>
  const mode = field === "name" ? prefs.nameDisplay : prefs.effectLanguage
  const showOriginal =
    mode !== "translated" || !value.translated || value.selection?.["basis"] === "jp_source"
  const showTranslated = mode !== "original" && value.translated !== undefined
  const originLabel =
    value.translation?.["origin"] === "official"
      ? t("card.translationOrigin.official")
      : value.translation?.["origin"] === "project"
        ? t("card.translationOrigin.project")
        : t("card.translationOrigin.machine")
  const authorityLabel =
    value.translation?.["authority"] === "sve_official"
      ? t("card.translationAuthority.sve_official")
      : value.translation?.["authority"] === "digital_official"
        ? t("card.translationAuthority.digital_official")
        : t("card.translationAuthority.unofficial")
  return (
    <section className="mt-3 space-y-2" aria-label={t(`card.field.${field}`)}>
      <h3 className="text-14 font-semibold">{t(`card.field.${field}`)}</h3>
      {value.printedState === "derived" && <p>{t("card.printedDerived")}</p>}
      {value.printedState === "unknown" && <p>{t("card.printedUnconfirmed")}</p>}
      {showOriginal && (
        <AnnotatedText value={value.original} context={value} emphasis={prefs.termEmphasis} />
      )}
      {showTranslated && (
        <>
          <AnnotatedText value={value.translated} context={value} emphasis={prefs.termEmphasis} />
          <p className="text-12 text-text-3">
            {originLabel} · {authorityLabel}
          </p>
          {value.translation?.["low_confidence"] === true && (
            <p>{t("card.translationProofreading")}</p>
          )}
          {value.selection?.["basis"] === "jp_source" && value.source && (
            <details>
              <summary>{t("card.jpSource")}</summary>
              <AnnotatedText value={value.source} context={value} emphasis={prefs.termEmphasis} />
            </details>
          )}
        </>
      )}
      {mode !== "original" &&
        !value.translated &&
        stringValue(value.original.unit["lang"]) !== language && (
          <p>{t("card.translationMissing")}</p>
        )}
      <button
        type="button"
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
