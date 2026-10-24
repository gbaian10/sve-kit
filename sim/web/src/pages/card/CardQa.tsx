import { useEffect, useId, useState } from "react"
import { useTranslation } from "react-i18next"

import { AnnotatedText } from "../../components/card/AnnotatedText"
import { OriginalBox } from "../../components/card/OriginalBox"
import { useTextLanguage } from "../../components/card/text-language"
import { TranslationSourceLine } from "../../components/card/TranslationSourceLine"
import { Button } from "../../components/ui/Button"
import { Link } from "../../components/ui/Link"
import { Notice } from "../../components/ui/Notice"
import { rovingRadioKeyDown } from "../../components/ui/useRovingRadio"
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
import { type FieldDisplay, usePrefs } from "../../settings"
import { type CardQa as CardQaResult, loadCardQa, type QaEntry } from "./qa-data"

const PRIMARY = "text-15 leading-effect"
const SECONDARY = "text-14 leading-effect text-text-2"

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
  mode,
  sourceLine,
}: {
  readonly client: SnapshotClient
  readonly edition: Region
  readonly label: string
  readonly state: TextState
  readonly language: string
  readonly mode: Exclude<FieldDisplay, "none">
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
    const translated = mode === "original" ? undefined : value.translated
    const foreign =
      mode !== "original" && !translated && stringValue(value.original.unit["lang"]) !== language
    const annotated = (text: typeof value.original, className: string) => (
      <AnnotatedText
        value={text}
        context={value}
        emphasis={prefs.termEmphasis}
        client={client}
        edition={edition}
        className={className}
      />
    )
    body = (
      <div className="space-y-1">
        {annotated(translated ?? value.original, PRIMARY)}
        {foreign && <p className="text-12 text-text-3">{t("qa.untranslated")}</p>}
        {mode === "both" && translated && (
          <OriginalBox>{annotated(value.original, SECONDARY)}</OriginalBox>
        )}
        {translated && sourceLine && <TranslationSourceLine value={value} proofreading={false} />}
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
  mode,
}: {
  readonly client: SnapshotClient
  readonly edition: Region
  readonly entry: QaEntry
  readonly mode: Exclude<FieldDisplay, "none">
}) {
  const { t } = useTranslation()
  const language = useTextLanguage()
  const question = useQaText(client, entry.versionId, "question", language)
  const answer = useQaText(client, entry.versionId, "answer", language)
  const values = [question, answer].map((state) => (state.phase === "ready" ? state.value : null))
  const qualities = values.flatMap((value) => qualityOf(value) ?? [])
  // One source line covers both texts unless their translations differ in kind or confidence.
  const shared = qualities.length > 0 && qualities.every((quality) => quality === qualities[0])
  const translated = mode === "original" ? [] : values.filter((value) => value?.translated)
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
        mode={mode}
        sourceLine={!shared}
      />
      <QaLine
        client={client}
        edition={edition}
        label={t("qa.answer")}
        state={answer}
        language={language}
        mode={mode}
        sourceLine={!shared}
      />
      {shared && translated[0] && (
        <TranslationSourceLine value={translated[0]} proofreading={false} />
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
  mode,
}: {
  readonly client: SnapshotClient
  readonly cardId: string
  readonly edition: Region
  readonly heading?: string
  readonly mode: Exclude<FieldDisplay, "none">
}) {
  const { t } = useTranslation()
  const snapshot: LoadedSnapshot | null = client.snapshot()
  const idPrefix = useId()
  const [attempt, setAttempt] = useState(0)
  const [selection, setSelection] = useState<{ readonly key: string; readonly index: number }>()
  const key = canonicalText([snapshot?.manifestHash ?? null, cardId, edition, attempt])
  const [loaded, setLoaded] = useState<Loaded>()
  const dataAsOf = snapshot ? stringValue(snapshot.manifest["data_as_of"]) : ""
  useEffect(() => {
    if (!snapshot) return
    let active = true
    const queryDate = stringValue(snapshot.manifest["data_as_of"])
    void loadCardQa(client, snapshot, cardId, edition, queryDate).then(
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
  else if (current.result.coverage === "absent" || current.result.coverage === "unknown")
    body = <p className="text-13 text-text-2">{t("qa.noneAsOf", { date: dataAsOf })}</p>
  else {
    const { entries } = current.result
    const selectedIndex = selection?.key === key ? Math.min(selection.index, entries.length - 1) : 0
    const tabValues = entries.map((_, index) => String(index))
    body = (
      <>
        <div className="max-w-full min-w-0 overflow-x-auto overscroll-x-contain">
          <div
            role="tablist"
            aria-label={heading ?? t("qa.heading")}
            className="flex w-max min-w-full gap-1 border-b border-border"
          >
            {entries.map((entry, index) => {
              const value = String(index)
              const selected = index === selectedIndex
              return (
                <button
                  key={entry.versionId}
                  type="button"
                  role="tab"
                  id={`${idPrefix}-${value}`}
                  aria-selected={selected}
                  aria-controls={`${idPrefix}-panel-${value}`}
                  tabIndex={selected ? 0 : -1}
                  onClick={() => {
                    setSelection({ key, index })
                  }}
                  onKeyDown={(event) => {
                    rovingRadioKeyDown(
                      event,
                      tabValues,
                      String(selectedIndex),
                      (next) => {
                        setSelection({ key, index: Number(next) })
                      },
                      idPrefix,
                    )
                  }}
                  className={`-mb-px h-10 shrink-0 border-b-2 px-3 text-13 font-semibold whitespace-nowrap focus-visible:outline-2 focus-visible:outline-offset-2 ${selected ? "border-text-1 text-text-1" : "border-transparent text-text-2 hover:text-text-1"}`}
                >
                  {entry.number || t("qa.tabFallback", { count: index + 1 })}
                </button>
              )
            })}
          </div>
        </div>
        {entries.map((entry, index) => {
          const value = String(index)
          return (
            <div
              key={entry.versionId}
              role="tabpanel"
              id={`${idPrefix}-panel-${value}`}
              aria-labelledby={`${idPrefix}-${value}`}
              // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- Static tabpanel content needs a keyboard entry point.
              tabIndex={0}
              hidden={index !== selectedIndex}
            >
              <ul className="space-y-3">
                <QaItem client={client} edition={edition} entry={entry} mode={mode} />
              </ul>
            </div>
          )
        })}
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
  const { qaDisplay } = usePrefs()
  if (qaDisplay === "none") return null
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
          mode={qaDisplay}
          {...(showOther ? { heading: t(`options.edition.${value}`) } : {})}
        />
      ))}
    </section>
  )
}
