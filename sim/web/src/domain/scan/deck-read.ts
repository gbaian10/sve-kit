import type { CardMatch, ScanCard } from "./search"
import type { Quad } from "./tensor"

/** How far below its own best guess a card may be taken from the rest of the same deck. */
export const DECK_GUESS_GAP = 0.06
/** A sideways reading wins only when it is clearly better than the upright one. */
const SIDEWAYS_LEAD = 0.02

export type DeckCardStatus = "ok" | "guess" | "unsure"

export interface DeckCard {
  readonly quad: Quad
  /** The straightened card as RGBA, for the picker. */
  readonly crop: Uint8ClampedArray
  /** Candidates, best first. */
  readonly alts: readonly CardMatch[]
  readonly pick: ScanCard | null
  readonly status: DeckCardStatus
  /** For a guess: how far it sits below the best candidate. */
  readonly gap: number
}

/** Upright and (for a card lying on its side) quarter-turned readings: the better one wins. */
export function chooseReading(
  upright: readonly CardMatch[],
  sideways: readonly CardMatch[] | null,
): "upright" | "sideways" {
  const side = sideways?.[0]
  const up = upright[0]
  return side && (!up || side.raw > up.raw + SIDEWAYS_LEAD) ? "sideways" : "upright"
}

/**
 * Cards the recogniser was unsure of take a candidate that is already recognised elsewhere in
 * the photo, when it scores within `gap` of their best: decks repeat cards.
 */
export function guessFromDeck(cards: readonly DeckCard[], gap = DECK_GUESS_GAP): DeckCard[] {
  const sure = new Set(
    cards.flatMap((card) => (card.status === "ok" && card.pick ? [card.pick.cardId] : [])),
  )
  return cards.map((card) => {
    const best = card.alts[0]
    if (card.status !== "unsure" || !best) return card
    const alt = card.alts.find(
      (match) => sure.has(match.card.cardId) && match.raw >= best.raw - gap,
    )
    return alt ? { ...card, pick: alt.card, status: "guess", gap: best.raw - alt.raw } : card
  })
}

export interface DeckGroup {
  readonly card: ScanCard
  /** Positions in the photo's reading order. */
  readonly at: readonly number[]
}

/** Picked cards with their copies, most copies first, then by first appearance. Unsure cards stay out. */
export function deckGroups(cards: readonly (DeckCard | null)[]): DeckGroup[] {
  const groups = new Map<string, { card: ScanCard; at: number[] }>()
  cards.forEach((card, at) => {
    if (!card || card.status === "unsure" || !card.pick) return
    const group = groups.get(card.pick.cardId) ?? { card: card.pick, at: [] }
    group.at.push(at)
    groups.set(card.pick.cardId, group)
  })
  return [...groups.values()].sort(
    (a, b) => b.at.length - a.at.length || (a.at[0] ?? 0) - (b.at[0] ?? 0),
  )
}

/** The deck as the import page reads it: a name line, then "count card-number name" per card. */
export function deckListText(groups: readonly DeckGroup[], name: string): string {
  if (groups.length === 0) return ""
  const lines = groups.map(
    (group) => `${String(group.at.length)} ${group.card.no} ${group.card.name}`,
  )
  return `# ${name}\n${lines.join("\n")}\n`
}
