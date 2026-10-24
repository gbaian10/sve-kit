import { Minus, Plus } from "lucide-react"
import { type ReactNode, useId } from "react"
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
import { prefsStore, type TextBoxKind, usePrefs } from "../../settings"
import { cn } from "../ui/cn"
import { AnnotatedText } from "./AnnotatedText"
import { useTextLanguage } from "./text-language"
import { TextPurposeStatus } from "./TextPurposeStatus"
import { TranslationSourceLine } from "./TranslationSourceLine"

// Effect text is the reason the page exists: 16px at line-height 1.75, full width (design 06).
const BODY = "text-16 leading-effect"
const TEXT_BOX =
  "rounded-block border border-border bg-surface-1 text-14 leading-effect text-text-2"

function TextBox({
  kind,
  heading,
  field,
  children,
}: {
  readonly kind?: TextBoxKind
  readonly heading: string
  readonly field?: string
  readonly children: ReactNode
}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const contentId = useId()
  const expanded = kind === undefined || !prefs.collapsedText[kind]
  const Container = field ? "section" : "div"
  return (
    <Container
      className={cn(TEXT_BOX, !kind && "px-3 py-2.5", field && "mt-3")}
      aria-label={field ? heading : undefined}
      data-field={field}
    >
      <h3 className="text-12 leading-5 text-text-3" aria-label={heading}>
        {kind ? (
          <button
            type="button"
            className="flex w-full cursor-pointer items-center justify-between gap-3 rounded-control px-3 py-1.5 text-start"
            aria-expanded={expanded}
            aria-controls={contentId}
            aria-label={
              expanded
                ? t("card.collapseText", { field: heading })
                : t("card.expandText", { field: heading })
            }
            onClick={() => {
              const collapsedText = prefsStore.get().collapsedText
              prefsStore.set({ collapsedText: { ...collapsedText, [kind]: !collapsedText[kind] } })
            }}
          >
            <span>{heading}</span>
            {expanded ? (
              <Minus aria-hidden="true" size={14} />
            ) : (
              <Plus aria-hidden="true" size={14} />
            )}
          </button>
        ) : (
          heading
        )}
      </h3>
      <div id={contentId} hidden={!expanded}>
        <div className={cn("mt-1 space-y-2", kind && "px-3 pb-2.5")}>{children}</div>
      </div>
    </Container>
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
}) {
  const { t } = useTranslation()
  const { readiness, value } = state
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
    const translated = show === "translated" ? value.translated : undefined
    body = (
      <>
        <AnnotatedText
          value={translated ?? value.original}
          context={value}
          emphasis={emphasis}
          annotationReady={readiness.annotations === "ready" || readiness.annotations === "none"}
          client={client}
          edition={edition}
        />
        {translated && sourceLine && <TranslationSourceLine value={value} />}
        {show === "translated" &&
          noteMissing &&
          !translated &&
          stringValue(value.original.unit["lang"]) !== language && (
            <p className="text-13 text-text-2">{t("card.translationMissing")}</p>
          )}
      </>
    )
  }
  return (
    <div className={cn("space-y-1", rule && "border-t border-border pt-2")} data-section>
      {label !== undefined && <p className="text-13 text-text-3">{label}</p>}
      {body}
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
  readonly display?: NameDisplay | "none" | undefined
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
  const mode =
    display ??
    (field === "name"
      ? prefs.nameDisplay
      : field === "flavor"
        ? prefs.flavorDisplay
        : prefs.effectLanguage)
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
  // The original box: this printing's own text when it differs, else the owner's.
  let printedBody: ReactNode
  if (!printed)
    printedBody = (
      <>
        {effectOriginal}
        {sectionTexts("original")}
      </>
    )
  else if (printedText.readiness.basic === "pending")
    printedBody = (
      <p role="status" className="text-13">
        {t("card.annotationPending")}
      </p>
    )
  else if (printedText.readiness.basic === "error")
    printedBody = (
      <p role="alert" className="text-13">
        {printedText.readiness.error?.kind === "validation"
          ? t("card.textFailed")
          : t("card.textLoadFailed")}
      </p>
    )
  else if (!printedText.value) printedBody = <p className="text-13">{t("card.textUnavailable")}</p>
  else {
    const printedValue = printedText.value
    const printedBlank = stringValue(printedValue.original.unit["text"]) === ""
    printedBody = (
      <>
        {!printedBlank && (
          <AnnotatedText
            value={printedValue.original}
            context={printedValue}
            emphasis={prefs.termEmphasis}
            annotationReady={
              printedText.readiness.annotations === "ready" ||
              printedText.readiness.annotations === "none"
            }
            client={client}
            edition={edition}
          />
        )}
        {sectionTexts("original", printedSections, printedSectionStates, printedBlank)}
      </>
    )
  }
  const original = <div className="space-y-2">{printedBody}</div>
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
            {value.translated ? (
              <AnnotatedText
                value={value.translated}
                context={value}
                emphasis={prefs.termEmphasis}
                annotationReady={annotationReady}
                client={client}
                edition={edition}
              />
            ) : (
              effectOriginal
            )}
            {value.translated && !sharedLine && <TranslationSourceLine value={value} />}
            {effectMissing}
            {sectionTexts("translated")}
          </div>
          {boxMissing}
        </>
      ) : (
        <div className={BODY}>{original}</div>
      )}
    </>
  )
  if (boxed) {
    if (showOriginal)
      return (
        <section className="mt-3 space-y-2" aria-label={heading} data-field={field}>
          <TextBox heading={heading}>{content}</TextBox>
          <TextBox kind={`${field}Original`} heading={t("card.originalText")}>
            {original}
          </TextBox>
        </section>
      )
    return (
      <TextBox heading={heading} field={field}>
        {content}
      </TextBox>
    )
  }
  return (
    <section className="mt-3 space-y-2" aria-label={heading} data-field={field}>
      <h3 className="text-13 font-semibold text-text-3">{heading}</h3>
      {content}
      {showOriginal && (
        <div className={cn(TEXT_BOX, "px-3 py-2.5")}>
          <p className="mb-1 text-12 text-text-3">{t("card.originalText")}</p>
          {original}
        </div>
      )}
    </section>
  )
}
