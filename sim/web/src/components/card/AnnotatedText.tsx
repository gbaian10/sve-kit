import { X } from "lucide-react"
import {
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useTranslation } from "react-i18next"

import { useImageIndex } from "../../app/snapshot"
import {
  type AnnotatedText as TextValue,
  arrayValue,
  type CardSummary,
  type JsonObject,
  type SelectedText,
  type SnapshotClient,
  stringValue,
  textRuns,
} from "../../data"
import { printingPath } from "../../domain/cardRoute"
import { displayName } from "../../domain/nameDisplay"
import { editionUrl, querySearch } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
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

/** What a reference does when followed; null leaves the text as it is (bold or not). */
type Action =
  | { readonly kind: "card"; readonly cards: readonly CardSummary[] }
  | { readonly kind: "trait"; readonly code: string }

/** One printing per linked card: the shown edition's, else whichever the snapshot holds. */
function representatives(cards: readonly CardSummary[], edition: Region): CardSummary[] {
  const byCard = new Map<string, CardSummary>()
  for (const card of cards) {
    const current = byCard.get(card.cardId)
    if (!current || (current.region !== edition && card.region === edition))
      byCard.set(card.cardId, card)
  }
  return [...byCard.values()]
}

// Only references a reader can follow get a control (maintainer, 2026-10-19); the rest stay text.
function actionOf(reference: JsonObject, context: SelectedText, edition: Region): Action | null {
  const kind = reference["kind"]
  if (kind !== "card_name" && kind !== "glossary") return null
  const id = reference[kind === "card_name" ? "term_id" : "key"]
  const concept = context.concepts.find((row) => row["id"] === id)
  if (!concept) return null
  if (kind === "card_name") {
    const cards = representatives(
      context.cards.filter((card) => arrayValue(concept["card_ids"]).includes(card.cardId)),
      edition,
    )
    return cards.length > 0 ? { kind: "card", cards } : null
  }
  if (concept["category"] !== "trait" || typeof id !== "string") return null
  // Trait concepts carry the vocabulary code the list filters by; an unlisted code stays text.
  const code = id.replace(/^term:trait\./u, "")
  const listed = context.vocabulary?.some((row) => row.kind === "trait" && row.code === code)
  return code !== id && listed ? { kind: "trait", code } : null
}

function LinkedCard({
  card,
  edition,
  images,
  size,
}: {
  readonly card: CardSummary
  readonly edition: Region
  readonly images: ReturnType<typeof useImageIndex>
  readonly size: "preview" | "row"
}) {
  const prefs = usePrefs()
  const name = displayName(card.name, currentUiLanguage(prefs.uiLanguage), prefs.nameDisplay)
  return (
    <>
      <CardImage
        summary={card}
        name={name.primary}
        images={images}
        alt="decorative"
        sizes={size === "preview" ? "96px" : "40px"}
        className={size === "preview" ? "w-24 shrink-0" : "w-10 shrink-0"}
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
    </>
  )
}

/** A hovered card-name reference and where its preview goes, relative to the text block. */
interface Preview {
  readonly index: number
  readonly card: CardSummary
  readonly left: number
  readonly top: number
}

/** A card-name reference: hover previews the card, a click goes to it or lists several. */
function CardReference({
  index,
  cards,
  tooltipId,
  onPreview,
  onList,
  children,
}: {
  readonly index: number
  readonly cards: readonly CardSummary[]
  /** The preview's id while this reference is the one showing it. */
  readonly tooltipId: string | undefined
  readonly onPreview: (preview: Preview | null) => void
  readonly onList: (trigger: HTMLElement) => void
  readonly children: ReactNode
}) {
  const single = cards.length === 1 ? cards[0] : undefined
  const underline = "cursor-pointer text-inherit underline decoration-dotted underline-offset-4"
  // The preview holds block elements, so it is positioned from here but rendered outside the <p>.
  const show = (event: { currentTarget: Element }) => {
    if (!single) return
    const anchor = event.currentTarget.getBoundingClientRect()
    const block = event.currentTarget.closest("[data-annotated]")?.getBoundingClientRect()
    onPreview({
      index,
      card: single,
      left: anchor.left - (block?.left ?? 0),
      top: anchor.bottom - (block?.top ?? 0),
    })
  }
  const hide = () => {
    onPreview(null)
  }
  const hovering = { onMouseEnter: show, onMouseLeave: hide, onFocus: show, onBlur: hide }
  return single ? (
    <Link
      to={printingPath(single)}
      className={cn(underline, "hover:no-underline")}
      aria-describedby={tooltipId}
      {...hovering}
    >
      {children}
    </Link>
  ) : (
    <button
      type="button"
      className={cn("inline", underline)}
      onClick={(event) => {
        onList(event.currentTarget)
      }}
      {...hovering}
    >
      {children}
    </button>
  )
}

function CardPreview({
  id,
  preview,
  client,
  edition,
}: {
  readonly id: string
  readonly preview: Preview
  readonly client: SnapshotClient
  readonly edition: Region
}) {
  const images = useImageIndex(client, true)
  const ref = useRef<HTMLDivElement>(null)
  const [left, setLeft] = useState(preview.left)
  // A reference near the end of a line would push the preview past the block: keep it inside.
  useLayoutEffect(() => {
    const node = ref.current
    const block = node?.closest("[data-annotated]")
    if (!node || !block) return
    setLeft(Math.max(0, Math.min(preview.left, block.clientWidth - node.offsetWidth)))
  }, [preview])
  return (
    <div
      ref={ref}
      id={id}
      role="tooltip"
      style={{ left, top: preview.top }}
      className="absolute z-20 mt-1 flex w-max max-w-full items-center gap-2.5 rounded-block border border-border bg-surface-1 p-2 shadow-lg"
    >
      <LinkedCard card={preview.card} edition={edition} images={images} size="preview" />
    </div>
  )
}

function CardList({
  cards,
  client,
  edition,
  onClose,
}: {
  readonly cards: readonly CardSummary[]
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
  return (
    <aside
      aria-label={t("card.reference.cardName")}
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
      <p className="text-12 text-text-3">{t("card.reference.cardName")}</p>
      <ul className="mt-1 flex flex-col gap-1">
        {cards.map((card) => (
          <li key={card.printingId}>
            <Link
              to={printingPath(card)}
              className="flex items-center gap-2.5 rounded-control p-1 text-text-1 hover:bg-surface-3 hover:no-underline"
            >
              <LinkedCard card={card} edition={edition} images={images} size="row" />
            </Link>
          </li>
        ))}
      </ul>
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
  /** The edition shown: lists filter in it, and a linked card of the other edition is tagged. */
  readonly edition: Region
  readonly className?: string
}) {
  const { t } = useTranslation()
  const tooltipId = useId()
  const [list, setList] = useState<{
    readonly cards: readonly CardSummary[]
    readonly trigger: HTMLElement
  } | null>(null)
  const [preview, setPreview] = useState<Preview | null>(null)
  // Closing the list hands focus back to the reference that opened it.
  const closeList = useCallback(() => {
    setList((open) => {
      open?.trigger.focus()
      return null
    })
  }, [])
  const Text = heading ? "h2" : "p"
  const text = stringValue(value.unit["text"])
  const segments = useMemo(
    () => layOut(text, annotationReady ? textRuns(value) : [{ text }]),
    [text, value, annotationReady],
  )
  return (
    <div className="relative" data-annotated>
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
          const action =
            annotationReady && segment.reference !== null
              ? actionOf(segment.reference, context, edition)
              : null
          if (action?.kind === "card")
            return (
              <CardReference
                key={index}
                index={index}
                cards={action.cards}
                tooltipId={preview?.index === index ? tooltipId : undefined}
                onPreview={setPreview}
                onList={(trigger) => {
                  setList({ cards: action.cards, trigger })
                }}
              >
                {content}
              </CardReference>
            )
          if (action?.kind === "trait")
            return (
              <Link
                key={index}
                to={editionUrl(
                  `/cards${querySearch({ ...DEFAULT_QUERY, traits: [action.code] })}`,
                  edition,
                )}
                title={t("card.reference.filterTrait")}
                className="border-b border-accent text-inherit hover:bg-accent-soft hover:no-underline"
              >
                {content}
              </Link>
            )
          return <span key={index}>{content}</span>
        })}
      </Text>
      {preview && (
        <CardPreview id={tooltipId} preview={preview} client={client} edition={edition} />
      )}
      {list && (
        <CardList cards={list.cards} client={client} edition={edition} onClose={closeList} />
      )}
    </div>
  )
}
