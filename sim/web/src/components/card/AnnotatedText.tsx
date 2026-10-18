import { X } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import { useImageIndex } from "../../app/snapshot"
import {
  type AnnotatedText as TextValue,
  arrayValue,
  canonicalText,
  type CardSummary,
  type JsonObject,
  type SelectedText,
  type SnapshotClient,
  stringValue,
  textRuns,
} from "../../data"
import { printingPath } from "../../domain/cardRoute"
import { displayName } from "../../domain/nameDisplay"
import type { Region } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { cn } from "../ui/cn"
import { Link } from "../ui/Link"
import { CardImage } from "./CardImage"
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

interface Entry {
  readonly kind: "card_name" | "glossary"
  readonly category: Category | undefined
  readonly cards: readonly CardSummary[]
  readonly explanations: SelectedText["explanations"]
}

/**
 * What a reference opens, or null when nothing public backs it yet: only confirmed card-name
 * targets and glossary terms with a public explanation become entries, so the text highlights
 * exactly the references a reader can follow and lights up more as the data fills in.
 */
function entryOf(reference: JsonObject, context: SelectedText): Entry | null {
  const kind = reference["kind"]
  if (kind !== "card_name" && kind !== "glossary") return null
  const concept = context.concepts.find(
    (row) => row["id"] === reference[kind === "card_name" ? "term_id" : "key"],
  )
  if (!concept) return null
  const category = CATEGORIES.find((value) => value === concept["category"])
  const cards =
    kind === "card_name"
      ? context.cards.filter((card) => arrayValue(concept["card_ids"]).includes(card.cardId))
      : []
  const explanations = arrayValue(concept["explanations"])
    .map((ref) =>
      context.explanations.find((value) => canonicalText(value.reference) === canonicalText(ref)),
    )
    .filter((value) => value !== undefined)
  if (cards.length === 0 && explanations.length === 0) return null
  return { kind, category, cards, explanations }
}

function LinkedCard({
  card,
  edition,
  images,
}: {
  readonly card: CardSummary
  readonly edition: Region
  readonly images: ReturnType<typeof useImageIndex>
}) {
  const prefs = usePrefs()
  const name = displayName(card.name, currentUiLanguage(prefs.uiLanguage), prefs.nameDisplay)
  return (
    <li>
      <Link
        to={printingPath(card)}
        className="flex items-center gap-2.5 rounded-control p-1 text-text-1 hover:bg-surface-3 hover:no-underline"
      >
        <CardImage
          summary={card}
          name={name.primary}
          images={images}
          alt="decorative"
          sizes="40px"
          className="w-10 shrink-0"
        />
        <span className="flex min-w-0 flex-col leading-tight">
          <span lang={name.primary.lang} className="text-14 font-medium">
            {name.primary.text}
          </span>
          {name.secondary && (
            <span lang={name.secondary.lang} className="text-12 text-text-2">
              {name.secondary.text}
            </span>
          )}
          <span className="font-mono text-12 text-text-3">
            {card.cardNo}
            {card.region !== edition && ` · ${card.region.toUpperCase()}`}
          </span>
        </span>
      </Link>
    </li>
  )
}

function ReferenceEntry({
  entry,
  client,
  edition,
  onClose,
}: {
  readonly entry: Entry
  readonly client: SnapshotClient
  readonly edition: Region
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  const images = useImageIndex(client, true)
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) {
        event.preventDefault()
        onClose()
      }
    }
    document.addEventListener("keydown", onKeyDown)
    return () => {
      document.removeEventListener("keydown", onKeyDown)
    }
  }, [onClose])
  const heading =
    entry.kind === "card_name"
      ? t("card.reference.cardName")
      : entry.category
        ? t(`card.reference.category.${entry.category}`)
        : undefined
  return (
    <aside
      aria-label={t("card.annotationReference")}
      className="relative mt-2 rounded-block bg-surface-2 p-3 pr-11"
    >
      <button
        type="button"
        aria-label={t("card.closeReference")}
        title={t("card.closeReference")}
        onClick={onClose}
        className="absolute top-1.5 right-1.5 flex size-8 items-center justify-center rounded-control text-text-2 hover:bg-surface-3"
      >
        <X className="size-4.5" aria-hidden="true" />
      </button>
      {heading !== undefined && <p className="text-12 text-text-3">{heading}</p>}
      {entry.cards.length > 0 && (
        <ul className="mt-1 flex flex-col gap-1">
          {entry.cards.map((card) => (
            <LinkedCard key={card.printingId} card={card} edition={edition} images={images} />
          ))}
        </ul>
      )}
      {entry.explanations.map((value) => (
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
    </aside>
  )
}

export function AnnotatedText({
  value,
  context,
  emphasis,
  heading = false,
  annotationReady = true,
  client,
  edition,
  className,
}: {
  readonly value: TextValue
  readonly context: SelectedText
  readonly emphasis: boolean
  /** Render the text as the page's card-name heading instead of a paragraph. */
  readonly heading?: boolean
  readonly annotationReady?: boolean
  /** Loads the small card images of linked cards. */
  readonly client: SnapshotClient
  /** The edition shown; a linked card of another edition is tagged. */
  readonly edition: Region
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
  const entry = annotationReady && reference ? entryOf(reference, context) : null
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
          const openable =
            annotationReady && segment.reference !== null && entryOf(segment.reference, context)
          return openable ? (
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
      {entry && (
        <ReferenceEntry
          entry={entry}
          client={client}
          edition={edition}
          onClose={() => {
            setReference(null)
          }}
        />
      )}
    </div>
  )
}
