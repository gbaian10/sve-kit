import {
  arrayValue,
  type CardSummary,
  type Catalog,
  objectValue,
  type Row,
  type SelectedText,
} from "../../data"
import type { Region } from "../../domain/search"

/** One printing per linked card: the shown edition's, else whichever the snapshot holds. */
export function representatives(cards: readonly CardSummary[], edition: Region): CardSummary[] {
  const byCard = new Map<string, CardSummary>()
  for (const card of cards) {
    const current = byCard.get(card.cardId)
    if (!current || (current.region !== edition && card.region === edition))
      byCard.set(card.cardId, card)
  }
  return [...byCard.values()]
}

// Every text of a value counts: an English card's own text may carry no annotation while its
// Japanese source and source sections do.
export function mentionedCards(
  values: readonly (SelectedText | null | undefined)[],
  edition: Region,
): CardSummary[] {
  const ids: string[] = []
  const seen = new Set<string>()
  const pool: CardSummary[] = []
  for (const value of values) {
    if (!value) continue
    pool.push(...value.cards)
    for (const text of [
      value.original,
      value.translated,
      value.source,
      ...(value.sourceSections ?? []),
    ]) {
      if (!text?.annotation) continue
      for (const raw of arrayValue(text.annotation["occurrences"])) {
        const reference = objectValue(objectValue(raw)["reference"])
        if (reference["kind"] !== "card_name") continue
        const concept = value.concepts.find((row) => row["id"] === reference["term_id"])
        for (const cardId of arrayValue(concept?.["card_ids"] ?? [])) {
          if (typeof cardId !== "string" || seen.has(cardId)) continue
          seen.add(cardId)
          ids.push(cardId)
        }
      }
    }
  }
  const byCard = new Map(representatives(pool, edition).map((card) => [card.cardId, card]))
  return ids.flatMap((id) => {
    const card = byCard.get(id)
    return card ? [card] : []
  })
}

/** What decides whether two officially related cards are an evolve pair. */
export interface CardKind {
  /** The front face's name as the shown edition prints it. */
  readonly name: string | undefined
  readonly evolve: boolean
  readonly advance: boolean
  readonly token: boolean
  readonly follower: boolean
}

/** The representative printing of a card in the shown edition, or undefined when absent. */
export function representativeOf(
  catalog: Catalog,
  cardId: string,
  edition: Region,
): CardSummary | undefined {
  const card = catalog.index.card(cardId)
  const view = arrayValue(card?.["regions"] ?? [])
    .map((raw) => raw as Row)
    .find((entry) => entry["region"] === edition)
  const defaultId = view?.["default_printing_id"]
  return typeof defaultId === "string" ? catalog.summary(defaultId) : undefined
}

const ordinal = (face: Row): number => (typeof face["ordinal"] === "number" ? face["ordinal"] : 0)

/** The displayed revisions of a card's faces in the shown edition, front first. */
export function cardKindOf(
  catalog: Catalog,
  cardId: string,
  edition: Region,
): CardKind | undefined {
  const revisions = [...catalog.index.facesOf(cardId)]
    .sort((a, b) => ordinal(a) - ordinal(b))
    .flatMap((face) => {
      const revision =
        typeof face["id"] === "string" ? catalog.index.displayRevision(face["id"], edition) : null
      return revision ? [revision] : []
    })
  const front = revisions[0]
  if (!front) return undefined
  const kinds = new Set(
    revisions.flatMap((revision) => arrayValue(revision["special_kinds"] ?? [])),
  )
  return {
    name: representativeOf(catalog, cardId, edition)?.name.original.text,
    evolve: kinds.has("evolve"),
    advance: kinds.has("advance"),
    token: kinds.has("token"),
    follower: front["type_code"] === "follower",
  }
}

const sameName = (a: string | undefined, b: string | undefined): boolean =>
  a !== undefined && b !== undefined && a.trim() === b.trim()

/**
 * An official relation is an evolve pair when exactly one side is the evolve card and both carry
 * the same name, or when exactly one side is an advance card and the other is a plain follower
 * (advance cards are renamed, so the name cannot decide). Tokens, evolve-to-evolve rows and
 * same-name cards of other types are references.
 */
export function isEvolvePair(a: CardKind, b: CardKind): boolean {
  if (a.evolve !== b.evolve) return sameName(a.name, b.name)
  if (a.advance !== b.advance) {
    const base = a.advance ? b : a
    return base.follower && !base.token && !base.evolve && !base.advance
  }
  return false
}

export type EvolveDirection = "from" | "into"

/** An evolve or advance card lists what it evolves from; any other card lists what it evolves into. */
export const evolveDirection = (kind: CardKind): EvolveDirection =>
  kind.evolve || kind.advance ? "from" : "into"

export interface RelatedGroups {
  /** The evolve pairs of this card, all in the one direction its kind allows. */
  readonly evolve: {
    readonly direction: EvolveDirection
    readonly cards: readonly CardSummary[]
  }
  /** Cards this card's text names. */
  readonly mentions: readonly CardSummary[]
  /** The remaining official relations. */
  readonly otherOfficial: readonly CardSummary[]
}

const byCardNo = (a: CardSummary, b: CardSummary): number =>
  a.cardNo.localeCompare(b.cardNo, "en", { numeric: true }) ||
  a.printingId.localeCompare(b.printingId, "en")

// A card appears once: evolve pairs first, then the text's mentions, then the remaining official rows.
export function relatedGroups({
  cardId,
  related,
  mentions,
  representative,
  kind,
}: {
  readonly cardId: string
  /** Official `card_related` rows starting at this card. */
  readonly related: readonly Row[]
  readonly mentions: readonly CardSummary[]
  readonly representative: (cardId: string) => CardSummary | undefined
  readonly kind: (cardId: string) => CardKind | undefined
}): RelatedGroups {
  const self = kind(cardId)
  const seen = new Set<string>([cardId])
  const evolve: CardSummary[] = []
  const official: CardSummary[] = []
  for (const row of related) {
    const target = row["to_card_id"]
    if (typeof target !== "string" || seen.has(target)) continue
    const card = representative(target)
    if (!card) continue
    seen.add(target)
    const other = kind(target)
    if (self && other && isEvolvePair(self, other)) evolve.push(card)
    else official.push(card)
  }
  const mentioned: CardSummary[] = []
  const mentionedIds = new Set<string>()
  for (const card of mentions) {
    if (card.cardId === cardId || mentionedIds.has(card.cardId)) continue
    if (evolve.some((entry) => entry.cardId === card.cardId)) continue
    mentionedIds.add(card.cardId)
    mentioned.push(card)
  }
  return {
    evolve: { direction: self ? evolveDirection(self) : "into", cards: evolve.sort(byCardNo) },
    mentions: mentioned,
    otherOfficial: official.filter((card) => !mentionedIds.has(card.cardId)).sort(byCardNo),
  }
}
