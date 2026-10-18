import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import {
  type AnnotatedText as TextValue,
  arrayValue,
  canonicalText,
  type JsonObject,
  objectValue,
  type SelectedText,
  stringValue,
  textRuns,
} from "../../data"
import { printingPath } from "../../domain/cardRoute"
import { cn } from "../ui/cn"
import { Link } from "../ui/Link"
import { SymbolIcon } from "./InlineText"
import { inlinePieces, type SymbolPiece } from "./textSymbols"

interface Segment {
  readonly text: string
  readonly reference: JsonObject | null
  readonly bold: boolean
  readonly symbol?: SymbolPiece
}

// A marker may contain an annotation range; draw its icon once and preserve that explanation.
function layOut(
  text: string,
  runs: readonly {
    readonly text: string
    readonly reference?: JsonObject | null
    readonly bold?: boolean | null
  }[],
): readonly Segment[] {
  let offset = 0
  const spans = runs.map((run) => {
    const start = offset
    offset += run.text.length
    return { start, end: offset, reference: run.reference ?? null, bold: run.bold === true }
  })
  const segments: Segment[] = []
  for (const piece of inlinePieces(text)) {
    if (piece.kind === "symbol") {
      const span = spans.find(
        (candidate) =>
          candidate.reference !== null &&
          candidate.start < piece.end &&
          candidate.end > piece.start,
      )
      segments.push({
        text: piece.alt,
        reference: span?.reference ?? null,
        bold: false,
        symbol: piece,
      })
      continue
    }
    for (const span of spans) {
      const start = Math.max(span.start, piece.start)
      const end = Math.min(span.end, piece.end)
      if (start < end)
        segments.push({ text: text.slice(start, end), reference: span.reference, bold: span.bold })
    }
  }
  return segments
}

const CATEGORIES = ["keyword", "ability", "trait", "rule_term", "card_name"] as const
type Category = (typeof CATEGORIES)[number]

function categoryOf(concept: JsonObject | undefined): Category | undefined {
  const value = concept?.["category"]
  return CATEGORIES.find((category) => category === value)
}

/** What one reference opens: its kind, the vocabulary label, explanations, and linked cards. */
function ReferenceEntry({
  reference,
  context,
  onClose,
}: {
  readonly reference: JsonObject
  readonly context: SelectedText
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  const kind = reference["kind"]
  const concept = context.concepts.find(
    (row) => row["id"] === reference[kind === "card_name" ? "term_id" : "key"],
  )
  const vocabulary =
    kind === "vocabulary"
      ? context.vocabulary?.find(
          (row) =>
            row.kind === arrayValue(reference["key"])[0] &&
            row.code === arrayValue(reference["key"])[1],
        )
      : undefined
  const explanations = concept
    ? arrayValue(concept["explanations"])
        .map((ref) =>
          context.explanations.find(
            (value) => canonicalText(value.reference) === canonicalText(ref),
          ),
        )
        .filter((value) => value !== undefined)
    : []
  const category = categoryOf(concept)
  // Only confirmed concept-to-card relations link; an empty list is an honest "not linked yet".
  const cards = concept
    ? context.cards.filter((card) => arrayValue(concept["card_ids"]).includes(card.cardId))
    : []
  const heading =
    kind === "card_name"
      ? t("card.reference.cardName")
      : kind === "vocabulary"
        ? t("card.reference.vocabulary")
        : category
          ? t(`card.reference.category.${category}`)
          : undefined
  return (
    <aside
      aria-label={t("card.annotationReference")}
      className="mt-2 rounded-block bg-surface-2 p-3"
    >
      {heading !== undefined && <p className="text-12 text-text-3">{heading}</p>}
      {vocabulary && (
        <p lang={stringValue(objectValue(vocabulary.unit)["lang"])}>
          {stringValue(vocabulary.unit["text"])}
        </p>
      )}
      {kind === "card_name" &&
        (cards.length > 0 ? (
          <ul className="flex flex-col gap-1">
            {cards.map((card) => (
              <li
                key={`${card.region}/${card.cardNo}`}
                className="flex flex-wrap items-baseline gap-x-2"
              >
                <Link to={printingPath(card)} className="font-mono text-14">
                  {card.cardNo}
                </Link>
                {card.name && (
                  <span lang={card.name.lang} className="text-14">
                    {card.name.text}
                  </span>
                )}
                <span className="text-12 text-text-3">{card.region.toUpperCase()}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p>{t("card.reference.unlinked")}</p>
        ))}
      {explanations.map((value) => (
        <div key={canonicalText(value.reference)}>
          {value.reference["kind"] === "ruling_revision" && (
            <>
              <p>
                {t("card.projectRuling")} ·{" "}
                {value.row["strength"] === "official"
                  ? t("card.rulingOfficial")
                  : value.row["strength"] === "generalized"
                    ? t("card.rulingGeneralized")
                    : value.row["strength"] === "inferred"
                      ? t("card.rulingInferred")
                      : t("card.rulingUndecided")}
              </p>
              {value.row["review_state"] === "needs_review" && <p>{t("card.rulingNeedsReview")}</p>}
              {value.row["review_state"] === "superseded" && <p>{t("card.rulingSuperseded")}</p>}
            </>
          )}
          <p lang={stringValue(value.unit["lang"])} className="whitespace-pre-wrap">
            {stringValue(value.unit["text"])}
          </p>
        </div>
      ))}
      {explanations.length === 0 && kind !== "card_name" && kind !== "vocabulary" && (
        <p>{t("card.noExplanation")}</p>
      )}
      <button type="button" onClick={onClose}>
        {t("card.closeReference")}
      </button>
    </aside>
  )
}

export function AnnotatedText({
  value,
  context,
  emphasis,
  heading = false,
  annotationReady = true,
  className,
}: {
  readonly value: TextValue
  readonly context: SelectedText
  readonly emphasis: boolean
  /** Render the text as the page's card-name heading instead of a paragraph. */
  readonly heading?: boolean
  readonly annotationReady?: boolean
  readonly className?: string
}) {
  const { t } = useTranslation()
  const [reference, setReference] = useState<JsonObject | null>(null)
  const Text = heading ? "h2" : "p"
  const text = stringValue(value.unit["text"])
  const segments = useMemo(
    () => layOut(text, annotationReady ? textRuns(value) : [{ text }]),
    [text, value, annotationReady],
  )
  return (
    <div>
      <Text lang={stringValue(value.unit["lang"])} className={cn("whitespace-pre-wrap", className)}>
        {segments.map((segment, index) => {
          const content =
            segment.symbol !== undefined ? (
              <SymbolIcon piece={segment.symbol} />
            ) : emphasis && segment.bold ? (
              <strong>{segment.text}</strong>
            ) : (
              segment.text
            )
          return segment.reference ? (
            <button
              key={index}
              type="button"
              className="inline cursor-pointer text-inherit underline decoration-dotted underline-offset-4"
              title={t("card.annotationReference")}
              onClick={() => {
                setReference(segment.reference)
              }}
            >
              {content}
            </button>
          ) : (
            <span key={index}>{content}</span>
          )
        })}
      </Text>
      {annotationReady && reference && (
        <ReferenceEntry
          reference={reference}
          context={context}
          onClose={() => {
            setReference(null)
          }}
        />
      )}
    </div>
  )
}
