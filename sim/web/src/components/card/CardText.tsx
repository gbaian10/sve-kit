import { type ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { type AnnotatedTextState, useAnnotatedText, useAnnotatedTexts } from "../../app/snapshot"
import {
  type AnnotatedTextResolver,
  canonicalText,
  type JsonObject,
  type SelectedText,
  type SnapshotClient,
  stringValue,
} from "../../data"
import type { NameDisplay } from "../../domain/nameDisplay"
import type { Region } from "../../domain/search"
import { usePrefs } from "../../settings"
import { cn } from "../ui/cn"
import { AnnotatedText } from "./AnnotatedText"
import { OriginalBox } from "./OriginalBox"
import { useTextLanguage } from "./text-language"
import { TextPurposeStatus } from "./TextPurposeStatus"
import { TranslationSourceLine } from "./TranslationSourceLine"

// Effect text is the reason the page exists: 16px at line-height 1.75, full width (design 06).
const BODY = "text-16 leading-effect"
const TEXT_BOX =
  "rounded-block border border-border bg-surface-1 text-14 leading-effect text-text-2"

function TextBox({
  heading,
  field,
  children,
}: {
  readonly heading: string
  readonly field: string
  readonly children: ReactNode
}) {
  return (
    <section className={cn(TEXT_BOX, "mt-3 px-3 py-2.5")} aria-label={heading} data-field={field}>
      <h3 className="text-12 leading-5 text-text-3">{heading}</h3>
      <div className="mt-1 space-y-2">{children}</div>
    </section>
  )
}

/** The origin, authority and confidence a source line reports. */
function quality(value: SelectedText | null): string | undefined {
  const translation = value?.translation
  return translation
    ? canonicalText([
        translation["origin"] ?? null,
        translation["authority"] ?? null,
        translation["low_confidence"] ?? null,
      ])
    : undefined
}

/** How many lines a translation and its original share: their own count when equal, else 1. */
function sharedLines(
  translated: { readonly unit: JsonObject } | undefined,
  original: { readonly unit: JsonObject } | undefined,
): number {
  if (!translated || !original) return 1
  const count = (text: { readonly unit: JsonObject }) =>
    stringValue(text.unit["text"]).split("\n").length
  return count(translated) === count(original) ? count(translated) : 1
}

/**
 * A translation followed by its original in a box, line by line when `lines` > 1: line i of the
 * translation is line i of the original (the build derives the translation per source line).
 */
function PairedLines({
  lines,
  translated,
  original,
}: {
  readonly lines: number
  readonly translated: (line?: number) => ReactNode
  /** null leaves the translation without a box. */
  readonly original: ((line?: number) => ReactNode) | null
}) {
  if (lines <= 1)
    return (
      <>
        {translated()}
        {original && <OriginalBox>{original()}</OriginalBox>}
      </>
    )
  return Array.from({ length: lines }, (_, line) => (
    <div key={line} className="space-y-2" data-line={line}>
      {translated(line)}
      {original && <OriginalBox>{original(line)}</OriginalBox>}
    </div>
  ))
}

function OriginalText({
  state,
  line,
  emphasis,
  client,
  edition,
}: {
  readonly state: AnnotatedTextState
  readonly line?: number | undefined
  readonly emphasis: boolean
  readonly client: SnapshotClient
  readonly edition: Region
}) {
  const { t } = useTranslation()
  const { readiness, value } = state
  if (readiness.basic === "pending")
    return (
      <p role="status" className="text-13">
        {t("card.annotationPending")}
      </p>
    )
  if (readiness.basic === "error")
    return (
      <p role="alert" className="text-13">
        {readiness.error?.kind === "validation" ? t("card.textFailed") : t("card.textLoadFailed")}
      </p>
    )
  if (!value) return <p className="text-13">{t("card.textUnavailable")}</p>
  return (
    <AnnotatedText
      value={value.original}
      context={value}
      emphasis={emphasis}
      annotationReady={readiness.annotations === "ready" || readiness.annotations === "none"}
      client={client}
      edition={edition}
      line={line}
    />
  )
}

/** A section printed under the effect, separated from it by a rule like on the card face. */
function SectionText({
  state,
  label,
  show,
  language,
  emphasis,
  sourceLine,
  rule,
  noteMissing,
  pair,
  client,
  edition,
}: {
  readonly client: SnapshotClient
  readonly edition: Region
  readonly state: AnnotatedTextState
  readonly label: string | undefined
  readonly show: "translated" | "original"
  readonly language: string
  readonly emphasis: boolean
  readonly sourceLine: boolean
  readonly rule: boolean
  /** Say when this section lacks a translation; off when the whole box reports it once. */
  readonly noteMissing: boolean
  /**
   * Put the original in a box under the translation: `self` boxes this section's own original
   * (only when it has a translation), a state boxes that state's original (the printed wording).
   */
  readonly pair?: "self" | AnnotatedTextState | undefined
}) {
  const { t } = useTranslation()
  const { readiness, value } = state
  const translated = show === "translated" ? value?.translated : undefined
  const boxed = pair === "self" ? (translated ? state : undefined) : pair
  let body: ReactNode
  if (readiness.basic === "pending")
    body = (
      <p role="status" className="text-13">
        {t("card.annotationPending")}
      </p>
    )
  else if (readiness.basic === "error")
    body = (
      <p role="alert" className="text-13">
        {readiness.error?.kind === "validation" ? t("card.textFailed") : t("card.textLoadFailed")}
      </p>
    )
  else if (!value) body = <p className="text-13">{t("card.textUnavailable")}</p>
  else {
    const shown = translated ?? value.original
    body = (
      <PairedLines
        lines={boxed ? sharedLines(shown, boxed.value?.original) : 1}
        translated={(line) => (
          <AnnotatedText
            value={shown}
            context={value}
            emphasis={emphasis}
            annotationReady={readiness.annotations === "ready" || readiness.annotations === "none"}
            client={client}
            edition={edition}
            line={line}
          />
        )}
        original={
          boxed
            ? (line) => (
                <OriginalText
                  state={boxed}
                  line={line}
                  emphasis={emphasis}
                  client={client}
                  edition={edition}
                />
              )
            : null
        }
      />
    )
  }
  return (
    <div className={cn("space-y-1", rule && "border-t border-border pt-2")} data-section>
      {label !== undefined && <p className="text-13 text-text-3">{label}</p>}
      {body}
      {boxed && !value && (
        <OriginalBox>
          <OriginalText state={boxed} emphasis={emphasis} client={client} edition={edition} />
        </OriginalBox>
      )}
      {value && translated && sourceLine && <TranslationSourceLine value={value} />}
      {value &&
        show === "translated" &&
        noteMissing &&
        !translated &&
        stringValue(value.original.unit["lang"]) !== language && (
          <p className="text-13 text-text-2">{t("card.translationMissing")}</p>
        )}
    </div>
  )
}

export function CardText({
  client,
  owner,
  field,
  edition,
  sharedResolver,
  display,
  label,
  ordinal = null,
  printed,
}: {
  readonly sharedResolver?: AnnotatedTextResolver | null
  readonly client: SnapshotClient
  readonly edition: Region
  readonly owner: JsonObject
  readonly field: "name" | "effect" | "flavor"
  readonly ordinal?: number | null
  readonly display?: NameDisplay | undefined
  readonly label?: string | undefined
  /**
   * The text this printing actually prints when its own revision has no translation and the
   * translation of `owner` (the latest wording) stands in: the original box shows this one.
   */
  readonly printed?: JsonObject
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const language = useTextLanguage()
  const preference =
    field === "name"
      ? prefs.nameDisplay
      : field === "flavor"
        ? prefs.flavorDisplay
        : prefs.effectLanguage
  const mode = display ?? preference
  const { readiness, value } = useAnnotatedText(
    client,
    owner,
    field,
    language,
    edition,
    sharedResolver,
    ordinal,
  )
  const sections = value?.originalSections ?? []
  const sectionStates = useAnnotatedTexts(
    client,
    sections.map((section) => ({ owner, field: "section" as const, ordinal: section.ordinal })),
    language,
    edition,
    sharedResolver,
  )
  const printedText = useAnnotatedText(
    client,
    printed ?? owner,
    field,
    language,
    edition,
    sharedResolver,
    ordinal,
  )
  const printedSections = printed ? (printedText.value?.originalSections ?? []) : []
  const printedSectionStates = useAnnotatedTexts(
    client,
    printedSections.map((section) => ({
      owner: printed ?? owner,
      field: "section" as const,
      ordinal: section.ordinal,
    })),
    language,
    edition,
    sharedResolver,
  )
  if (mode === "none") return null
  if (readiness.basic === "pending") return <p role="status">{t("card.annotationPending")}</p>
  if (readiness.basic === "error")
    return (
      <p role="alert">
        {readiness.error?.kind === "validation" ? t("card.textFailed") : t("card.textLoadFailed")}
      </p>
    )
  if (!value) return <p>{t("card.textUnavailable")}</p>
  // Leaders and some tokens have no effect text at all: say so instead of an empty section.
  if (
    field === "effect" &&
    stringValue(value.original.unit["text"]) === "" &&
    !value.translated &&
    sections.length === 0
  )
    return (
      <p className="mt-3 rounded-control border border-dashed border-border-strong px-3 py-3 text-center text-14 text-text-2">
        {t("card.noEffect")}
      </p>
    )
  const annotationReady = readiness.annotations === "ready" || readiness.annotations === "none"
  // Each text falls back on its own when translating; a box with no translation at all says so once
  // at its end, while a partly translated box marks each untranslated text.
  const translating = mode !== "original"
  const anyTranslated =
    value.translated !== undefined || sectionStates.some((state) => state.value?.translated)
  const foreign = (text: { readonly unit: JsonObject } | undefined) =>
    text !== undefined && stringValue(text.unit["lang"]) !== language
  // A printing whose printed text differs shows it beside the latest wording even without a
  // translation (the latest wording itself is the "translation" in the card's own language).
  const showOriginal =
    translating &&
    (anyTranslated || printed !== undefined) &&
    (mode === "both" || value.selection?.["basis"] === "jp_source")
  const boxed = field !== "name"
  // The printed state describes the printed name and effect, never the flavor.
  const printedState = field === "flavor" ? undefined : value.printedState
  const heading = label ?? t(`card.field.${field}`)
  // A blank effect with sections (some tokens) prints only the sections.
  const blankEffect = stringValue(value.original.unit["text"]) === "" && !value.translated
  // One source line covers the effect and its sections unless a section's translation differs.
  const effectQuality = quality(value)
  const sharedLine = sectionStates.every(
    (state) => !state.value?.translated || quality(state.value) === effectQuality,
  )
  const sectionTexts = (
    show: "translated" | "original",
    list: typeof sections = sections,
    states: typeof sectionStates = sectionStates,
    blank: boolean = blankEffect,
    pairOf?: (section: (typeof sections)[number]) => "self" | AnnotatedTextState | undefined,
  ) =>
    list.flatMap((section, index) => {
      const state = states[index]
      return state
        ? [
            <SectionText
              key={section.ordinal}
              state={state}
              label={section.label}
              show={show}
              language={language}
              emphasis={prefs.termEmphasis}
              sourceLine={!sharedLine}
              rule={index > 0 || !blank}
              noteMissing={anyTranslated}
              pair={pairOf?.(section)}
              client={client}
              edition={edition}
            />,
          ]
        : []
    })
  const effectOriginal = blankEffect ? null : (
    <AnnotatedText
      value={value.original}
      context={value}
      emphasis={prefs.termEmphasis}
      annotationReady={annotationReady}
      client={client}
      edition={edition}
    />
  )
  const missingNote = <p className="text-13 text-text-2">{t("card.translationMissing")}</p>
  const effectMissing =
    translating && anyTranslated && !blankEffect && !value.translated && foreign(value.original)
      ? missingNote
      : null
  const boxMissing =
    translating &&
    !anyTranslated &&
    ((!blankEffect && foreign(value.original)) ||
      sectionStates.some((state) => foreign(state.value?.original)))
      ? missingNote
      : null
  // The original text: this printing's own wording when it differs, else the owner's.
  let printedEffect: ReactNode
  if (!printed) printedEffect = effectOriginal
  else {
    printedEffect =
      printedText.value && stringValue(printedText.value.original.unit["text"]) === "" ? null : (
        <OriginalText
          state={printedText}
          emphasis={prefs.termEmphasis}
          client={client}
          edition={edition}
        />
      )
  }
  const printedBlank = printed
    ? stringValue(printedText.value?.original.unit["text"] ?? "") === ""
    : blankEffect
  const original = (
    <div className="space-y-2">
      {printedEffect}
      {sectionTexts(
        "original",
        printed ? printedSections : sections,
        printed ? printedSectionStates : sectionStates,
        printedBlank,
      )}
    </div>
  )
  // Original-only sections of the printed wording that the latest wording has no counterpart for.
  const printedOnly = printed
    ? printedSections.flatMap((section, index) => {
        const state = printedSectionStates[index]
        return !state || sections.some((own) => own.ordinal === section.ordinal)
          ? []
          : [{ section, state }]
      })
    : []
  // Pair each translated text with its original by the card's own units: the effect, then each
  // section by ordinal; nothing is split or matched by position inside a unit.
  const pairOf = (section: (typeof sections)[number]) => {
    if (!showOriginal) return undefined
    if (!printed) return "self" as const
    const index = printedSections.findIndex((own) => own.ordinal === section.ordinal)
    return index < 0 ? undefined : printedSectionStates[index]
  }
  // The effect's own translated (or, untranslated, original) text and the original it pairs with.
  const shown = value.translated ?? (blankEffect ? undefined : value.original)
  const originalSide = printed ? printedText.value?.original : value.original
  const effectBox = showOriginal && (value.translated || printed) && printedEffect !== null
  const jpSource = value.selection?.["basis"] === "jp_source"
  const content = (
    <>
      {printedState === "derived" && <p className="text-13">{t("card.printedDerived")}</p>}
      {printedState === "unknown" && <p className="text-13">{t("card.printedUnconfirmed")}</p>}
      <TextPurposeStatus readiness={readiness} />
      {translating ? (
        <>
          {value.translated && sharedLine && <TranslationSourceLine value={value} />}
          {value.translated && jpSource && (
            <p className="text-12 text-text-3">{t("card.jpSource")}</p>
          )}
          {value.translated && printed !== undefined && (
            <p className="text-12 text-text-3" data-borrowed-translation>
              {t("card.wording.borrowedTranslation")}
            </p>
          )}
          <div className={cn(BODY, "space-y-2")}>
            {shown ? (
              <PairedLines
                // Flavor text is one passage: it keeps a single original box.
                lines={effectBox && field === "effect" ? sharedLines(shown, originalSide) : 1}
                translated={(line) => (
                  <AnnotatedText
                    value={shown}
                    context={value}
                    emphasis={prefs.termEmphasis}
                    annotationReady={annotationReady}
                    client={client}
                    edition={edition}
                    line={line}
                  />
                )}
                original={
                  effectBox
                    ? (line) =>
                        printed ? (
                          <OriginalText
                            state={printedText}
                            line={line}
                            emphasis={prefs.termEmphasis}
                            client={client}
                            edition={edition}
                          />
                        ) : (
                          <AnnotatedText
                            value={value.original}
                            context={value}
                            emphasis={prefs.termEmphasis}
                            annotationReady={annotationReady}
                            client={client}
                            edition={edition}
                            line={line}
                          />
                        )
                    : null
                }
              />
            ) : (
              effectBox && <OriginalBox>{printedEffect}</OriginalBox>
            )}
            {value.translated && !sharedLine && <TranslationSourceLine value={value} />}
            {effectMissing}
            {sectionTexts("translated", sections, sectionStates, blankEffect, pairOf)}
            {printedOnly.length > 0 && showOriginal && (
              <OriginalBox>
                {sectionTexts(
                  "original",
                  printedOnly.map((entry) => entry.section),
                  printedOnly.map((entry) => entry.state),
                  true,
                )}
              </OriginalBox>
            )}
          </div>
          {boxMissing}
        </>
      ) : (
        <div className={BODY}>{original}</div>
      )}
    </>
  )
  if (boxed)
    return (
      <TextBox heading={heading} field={field}>
        {content}
      </TextBox>
    )
  return (
    <section className="mt-3 space-y-2" aria-label={heading} data-field={field}>
      <h3 className="text-13 font-semibold text-text-3">{heading}</h3>
      {content}
    </section>
  )
}
