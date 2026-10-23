import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"

import { AnnotatedText } from "../../components/card/AnnotatedText"
import { useTextLanguage } from "../../components/card/text-language"
import { TranslationSourceLine } from "../../components/card/TranslationSourceLine"
import { Button } from "../../components/ui/Button"
import { Link } from "../../components/ui/Link"
import { Notice } from "../../components/ui/Notice"
import {
  canonicalText,
  type LoadedSnapshot,
  peekAnnotatedTextResolver,
  type SelectedText,
  sharedAnnotatedTextResolver,
  type SnapshotClient,
  SnapshotError,
  stringValue,
} from "../../data"
import type { Region } from "../../domain/search"
import { usePrefs } from "../../settings"
import { type CardQa as CardQaResult, loadCardQa, type QaEntry } from "./qa-data"

/** Entries shown before the rest folds behind a button. */
const FOLDED = 3

type QaField = "question" | "answer"

type TextState =
  | { readonly phase: "pending" }
  | { readonly phase: "error"; readonly validation: boolean; readonly retry: () => void }
  | { readonly phase: "ready"; readonly value: SelectedText | null }

const pointerOf = (versionId: string, field: QaField) => ({
  owner: { kind: "qa_version", id: versionId },
  field,
  ordinal: null,
})

/** One question or answer, resolved through the shared resolver like every other card text. */
function useQaText(
  client: SnapshotClient,
  versionId: string,
  field: QaField,
  language: string,
): TextState {
  const snapshot = client.snapshot()
  const [attempt, setAttempt] = useState(0)
  const key = canonicalText([versionId, field, language, snapshot?.manifestHash ?? null, attempt])
  const [settled, setSettled] = useState<{ key: string; state: TextState }>()
  useEffect(() => {
    if (!snapshot) return
    let active = true
    const settle = (state: TextState) => {
      if (active) setSettled({ key, state })
    }
    const pointer = pointerOf(versionId, field)
    const known = peekAnnotatedTextResolver(client)?.peek(pointer, language)
    if (known !== undefined) {
      settle({ phase: "ready", value: known })
      return
    }
    sharedAnnotatedTextResolver(client)
      .resolve(pointer, language)
      .then(
        (value) => {
          settle({ phase: "ready", value })
        },
        (error: unknown) => {
          settle({
            phase: "error",
            validation: error instanceof SnapshotError,
            retry: () => {
              setAttempt((value) => value + 1)
            },
          })
        },
      )
    return () => {
      active = false
    }
  }, [client, snapshot, key, versionId, field, language])
  return settled?.key === key ? settled.state : { phase: "pending" }
}

const qualityOf = (value: SelectedText | null | undefined) =>
  value?.translated
    ? canonicalText([
        value.translation?.["origin"] ?? null,
        value.translation?.["authority"] ?? null,
        value.translation?.["low_confidence"] ?? null,
      ])
    : undefined

function QaLine({
  client,
  edition,
  label,
  state,
  language,
  sourceLine,
}: {
  readonly client: SnapshotClient
  readonly edition: Region
  readonly label: string
  readonly state: TextState
  readonly language: string
  readonly sourceLine: boolean
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  let body
  if (state.phase === "pending")
    body = (
      <p role="status" className="text-13 text-text-2">
        {t("card.annotationPending")}
      </p>
    )
  else if (state.phase === "error")
    body = (
      <div className="space-y-2">
        <p role="alert" className="text-13 text-text-2">
          {state.validation ? t("card.textFailed") : t("card.textLoadFailed")}
        </p>
        {!state.validation && (
          <Button size="sm" onClick={state.retry}>
            {t("qa.retry")}
          </Button>
        )}
      </div>
    )
  else if (!state.value) body = <p className="text-13 text-text-2">{t("card.textUnavailable")}</p>
  else {
    const { value } = state
    const shown = value.translated ?? value.original
    const foreign = !value.translated && stringValue(value.original.unit["lang"]) !== language
    body = (
      <div className="space-y-1">
        <AnnotatedText
          value={shown}
          context={value}
          emphasis={prefs.termEmphasis}
          client={client}
          edition={edition}
          className="text-15 leading-effect"
        />
        {foreign && <p className="text-12 text-text-3">{t("qa.untranslated")}</p>}
        {sourceLine && <TranslationSourceLine value={value} />}
      </div>
    )
  }
  return (
    <div className="flex gap-2">
      <span className="mt-0.5 inline-flex size-6 shrink-0 items-center justify-center rounded-sm bg-surface-2 text-12 font-semibold text-text-2">
        {label}
      </span>
      <div className="min-w-0 flex-1">{body}</div>
    </div>
  )
}

function QaItem({
  client,
  edition,
  entry,
}: {
  readonly client: SnapshotClient
  readonly edition: Region
  readonly entry: QaEntry
}) {
  const { t } = useTranslation()
  const language = useTextLanguage()
  const question = useQaText(client, entry.versionId, "question", language)
  const answer = useQaText(client, entry.versionId, "answer", language)
  const values = [question, answer].map((state) => (state.phase === "ready" ? state.value : null))
  const qualities = values.flatMap((value) => qualityOf(value) ?? [])
  // One source line covers both texts unless their translations differ in kind or confidence.
  const shared = qualities.length > 0 && qualities.every((quality) => quality === qualities[0])
  const translated = values.filter((value) => value?.translated)
  return (
    <li
      aria-label={entry.number ?? undefined}
      className="space-y-2 rounded-block border border-border bg-surface-1 px-3 py-3"
      data-qa={entry.qaId}
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-12 text-text-2">
        {entry.number && <span className="font-mono font-semibold">{entry.number}</span>}
        {entry.url && (
          <Link to={entry.url} target="_blank" rel="noreferrer">
            {t("qa.official")}
          </Link>
        )}
      </div>
      <QaLine
        client={client}
        edition={edition}
        label={t("qa.question")}
        state={question}
        language={language}
        sourceLine={!shared}
      />
      <QaLine
        client={client}
        edition={edition}
        label={t("qa.answer")}
        state={answer}
        language={language}
        sourceLine={!shared}
      />
      {shared && translated[0] && <TranslationSourceLine value={translated[0]} />}
      {translated.length > 0 && (
        <details className="text-13">
          <summary className="cursor-pointer text-text-2">{t("qa.showOriginal")}</summary>
          <div className="mt-2 space-y-2 rounded-control border border-border px-3 py-2">
            {values.map(
              (value, index) =>
                value && (
                  <p key={index} className="text-14 leading-effect whitespace-pre-wrap text-text-2">
                    <span className="me-2 font-semibold text-text-3">
                      {t(index === 0 ? "qa.question" : "qa.answer")}
                    </span>
                    <span lang={stringValue(value.original.unit["lang"])}>
                      {stringValue(value.original.unit["text"])}
                    </span>
                  </p>
                ),
            )}
          </div>
        </details>
      )}
    </li>
  )
}

type Loaded =
  | { readonly key: string; readonly result: CardQaResult }
  | { readonly key: string; readonly failed: "network" | "validation" }

function QaGroup({
  client,
  cardId,
  edition,
  heading,
}: {
  readonly client: SnapshotClient
  readonly cardId: string
  readonly edition: Region
  readonly heading?: string
}) {
  const { t } = useTranslation()
  const snapshot: LoadedSnapshot | null = client.snapshot()
  const [attempt, setAttempt] = useState(0)
  const [expanded, setExpanded] = useState(false)
  const key = canonicalText([snapshot?.manifestHash ?? null, cardId, edition, attempt])
  const [loaded, setLoaded] = useState<Loaded>()
  useEffect(() => {
    if (!snapshot) return
    let active = true
    const today = new Date().toISOString().slice(0, 10)
    void loadCardQa(client, snapshot, cardId, edition, today).then(
      (result) => {
        if (active) setLoaded({ key, result })
      },
      (error: unknown) => {
        if (active)
          setLoaded({ key, failed: error instanceof SnapshotError ? "validation" : "network" })
      },
    )
    return () => {
      active = false
    }
  }, [client, snapshot, key, cardId, edition])
  const current = loaded?.key === key ? loaded : undefined
  let body
  if (!current)
    body = (
      <p role="status" className="text-13 text-text-2">
        {t("qa.loading")}
      </p>
    )
  else if ("failed" in current)
    body = (
      <Notice
        tone="danger"
        role="alert"
        actions={
          current.failed === "network" ? (
            <Button
              size="sm"
              onClick={() => {
                setAttempt(attempt + 1)
              }}
            >
              {t("qa.retry")}
            </Button>
          ) : undefined
        }
      >
        {current.failed === "network" ? t("qa.loadFailed") : t("qa.validationFailed")}
      </Notice>
    )
  else if (current.result.coverage === "absent")
    body = <p className="text-13 text-text-2">{t("qa.none")}</p>
  else if (current.result.coverage === "unknown")
    body = (
      <Notice tone="info" role="status">
        {t("qa.unknown")}
      </Notice>
    )
  else {
    // The list is complete here: every band that can name the card was read.
    const { entries } = current.result
    const visible = expanded ? entries : entries.slice(0, FOLDED)
    body = (
      <>
        <ul className="space-y-3">
          {visible.map((entry) => (
            <QaItem key={entry.versionId} client={client} edition={edition} entry={entry} />
          ))}
        </ul>
        {entries.length > FOLDED && (
          <Button
            size="sm"
            aria-expanded={expanded}
            onClick={() => {
              setExpanded(!expanded)
            }}
          >
            {expanded ? t("qa.collapse") : t("qa.showAll", { count: entries.length })}
          </Button>
        )}
      </>
    )
  }
  return (
    <div className="space-y-3" data-qa-edition={edition}>
      {heading && <h4 className="text-13 font-semibold text-text-3">{heading}</h4>}
      {body}
    </div>
  )
}

/** The card's official Q&A: the shown edition first, then the other one when comparison is on. */
export function CardQa({
  client,
  cardId,
  edition,
  showOther,
}: {
  readonly client: SnapshotClient
  readonly cardId: string
  readonly edition: Region
  readonly showOther: boolean
}) {
  const { t } = useTranslation()
  const editions: Region[] = showOther ? [edition, edition === "jp" ? "en" : "jp"] : [edition]
  return (
    <section aria-label={t("qa.heading")} className="mt-6 space-y-3">
      <h3 className="text-13 font-semibold text-text-3">{t("qa.heading")}</h3>
      {editions.map((value) => (
        <QaGroup
          key={value}
          client={client}
          cardId={cardId}
          edition={value}
          {...(showOther ? { heading: t(`options.edition.${value}`) } : {})}
        />
      ))}
    </section>
  )
}
