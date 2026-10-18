import { arrayValue, type CardSummary, objectValue, type Row, type SelectedText } from "../../data"
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

/**
 * The cards the texts' confirmed card-name references point at, in order of first appearance.
 * Every text of a resolved value (original, translation, Japanese source) counts, since an English
 * card's own text may carry no annotation while its Japanese source does.
 */
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
    for (const text of [value.original, value.translated, value.source]) {
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

export interface RelatedGroups {
  /** Official relations where either end is an evolve or advance card. */
  readonly evolve: readonly CardSummary[]
  /** Cards this card's text names. */
  readonly mentions: readonly CardSummary[]
  /** The remaining official relations. */
  readonly otherOfficial: readonly CardSummary[]
}

const EVOLVE_KINDS: readonly unknown[] = ["evolve", "advance"]

/** Whether a card's displayed revision marks it as an evolve or advance card. */
export function isEvolveKind(specialKinds: readonly unknown[]): boolean {
  return specialKinds.some((kind) => EVOLVE_KINDS.includes(kind))
}

const byCardNo = (a: CardSummary, b: CardSummary): number =>
  a.cardNo.localeCompare(b.cardNo, "en", { numeric: true }) ||
  a.printingId.localeCompare(b.printingId, "en")

/**
 * Splits a card's related cards into the groups the tabs show. A card appears once: the evolve
 * group takes precedence, then the text's own mentions, then the remaining official relations.
 * The official groups sort by card number; mentions keep the text's order.
 */
export function relatedGroups({
  cardId,
  related,
  mentions,
  representative,
  evolveKind,
}: {
  readonly cardId: string
  /** Official `card_related` rows starting at this card. */
  readonly related: readonly Row[]
  readonly mentions: readonly CardSummary[]
  /** The representative printing of a card in the shown edition, or undefined when absent. */
  readonly representative: (cardId: string) => CardSummary | undefined
  readonly evolveKind: (cardId: string) => boolean
}): RelatedGroups {
  const selfEvolve = evolveKind(cardId)
  const seen = new Set<string>([cardId])
  const evolve: CardSummary[] = []
  const official: CardSummary[] = []
  for (const row of related) {
    const target = row["to_card_id"]
    if (typeof target !== "string" || seen.has(target)) continue
    const card = representative(target)
    if (!card) continue
    seen.add(target)
    if (selfEvolve || evolveKind(target)) evolve.push(card)
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
    evolve: evolve.sort(byCardNo),
    mentions: mentioned,
    otherOfficial: official.filter((card) => !mentionedIds.has(card.cardId)).sort(byCardNo),
  }
}
