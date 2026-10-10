import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import {
  type AnnotatedTextResolver,
  arrayValue,
  canonicalText,
  createAnnotatedTextResolver,
  type JsonObject,
  objectValue,
  parseStrict,
  type SelectedText,
  type SnapshotClient,
  SnapshotError,
  stringValue,
} from "../../data"
import { usePrefs } from "../../settings"
import { AnnotatedText } from "./AnnotatedText"

export function CardText({
  client,
  owner,
  field,
  sharedResolver,
}: {
  readonly sharedResolver?: AnnotatedTextResolver | null
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
    () => sharedResolver ?? (snapshot ? createAnnotatedTextResolver(client) : null),
    [client, snapshot, sharedResolver],
  )
  const [loaded, setLoaded] = useState<{
    readonly snapshot: typeof snapshot
    readonly identity: string
    readonly value?: SelectedText | null
    readonly failed?: "validation" | "network"
  }>()
  useEffect(() => {
    if (!resolver) return
    let active = true
    const [currentOwner, currentField, currentLanguage] = arrayValue(parseStrict(identity))
    const pointer = {
      owner: objectValue(currentOwner),
      field: stringValue(currentField),
      ordinal: null,
    }
    void resolver.resolve(pointer, stringValue(currentLanguage)).then(
      (value) => {
        if (active) setLoaded({ snapshot, identity, value })
      },
      (error: unknown) => {
        if (active)
          setLoaded({
            snapshot,
            identity,
            failed: error instanceof SnapshotError ? "validation" : "network",
          })
      },
    )
    return () => {
      active = false
    }
  }, [resolver, snapshot, identity])
  if (loaded?.snapshot !== snapshot || loaded.identity !== identity)
    return <p role="status">{t("card.annotationPending")}</p>
  if (loaded.failed)
    return (
      <p role="alert">
        {loaded.failed === "validation" ? t("card.textFailed") : t("card.textLoadFailed")}
      </p>
    )
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
