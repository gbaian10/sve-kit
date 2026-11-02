import { totalCount } from "./model"
import type { Deck, DeckEntry } from "./types"

export interface TextLabels {
  readonly leader: string
  readonly main: string
  readonly evolve: string
  readonly side: string
}

/**
 * A deck list people read and paste: `# name`, then a `## heading` per part and one
 * "COUNT CARD-NUMBER NAME" line per card. Reading it back uses only count and card number.
 */
export function deckToText(
  deck: Deck,
  lookup: {
    readonly cardNoOf: (entry: DeckEntry) => string | undefined
    readonly nameOf: (cardId: string) => string | undefined
  },
  labels: TextLabels,
): string {
  const line = (entry: DeckEntry) =>
    `${String(entry.count)} ${lookup.cardNoOf(entry) ?? entry.cardId} ${lookup.nameOf(entry.cardId) ?? ""}`.trimEnd()
  const lines = [`# ${deck.name}`]
  if (deck.leader) lines.push("", `## ${labels.leader}`, line({ ...deck.leader, count: 1 }))
  for (const [label, entries] of [
    [labels.main, deck.main],
    [labels.evolve, deck.evolve],
    [labels.side, deck.side],
  ] as const) {
    if (entries.length === 0 && entries === deck.side) continue
    lines.push("", `## ${label} (${String(totalCount(entries))})`, ...entries.map(line))
  }
  return `${lines.join("\n")}\n`
}

interface TextCard {
  readonly line: number
  readonly count: number
  readonly cardNo: string
  /** Under a side-list heading; every other card goes where the card itself belongs. */
  readonly side: boolean
}

export interface ParsedDeckText {
  readonly name: string | undefined
  readonly cards: readonly TextCard[]
  /** Lines that are neither a heading nor a card, with their 1-based line number. */
  readonly unrecognized: readonly { readonly line: number; readonly text: string }[]
}

// Side-list headings in the three UI languages and common deck-site spellings.
const SIDE_HEADING = /候補|候选|maybe|side/iu
const CARD_NO = String.raw`([A-Za-z0-9]+-[^\s×xX]+)`
// "3 BP01-001 name", "3x BP01-001", "BP01-001 x3", "BP01-001 ×3", or a lone number for one copy.
const COUNT_FIRST = new RegExp(String.raw`^(\d{1,3})\s*[x×]?\s+${CARD_NO}`, "u")
const NUMBER_FIRST = new RegExp(String.raw`^${CARD_NO}\s*[x×]\s*(\d{1,3})(?:\s|$)`, "u")
const NUMBER_ONLY = new RegExp(String.raw`^${CARD_NO}$`, "u")

export function parseDeckText(text: string): ParsedDeckText {
  let name: string | undefined
  let side = false
  const cards: TextCard[] = []
  const unrecognized: { line: number; text: string }[] = []
  for (const [index, raw] of text.split(/\r?\n/u).entries()) {
    const line = raw.trim()
    const number = index + 1
    if (line === "") continue
    if (line.startsWith("##")) {
      side = SIDE_HEADING.test(line)
      continue
    }
    if (line.startsWith("#")) {
      name ??= line.slice(1).trim() || undefined
      continue
    }
    const countFirst = COUNT_FIRST.exec(line)
    const numberFirst = countFirst ? null : NUMBER_FIRST.exec(line)
    const only = countFirst || numberFirst ? null : NUMBER_ONLY.exec(line)
    const count = countFirst?.[1] ?? numberFirst?.[2] ?? (only ? "1" : undefined)
    const cardNo = countFirst?.[2] ?? numberFirst?.[1] ?? only?.[1]
    if (count === undefined || cardNo === undefined || Number(count) === 0)
      unrecognized.push({ line: number, text: line })
    else cards.push({ line: number, count: Number(count), cardNo, side })
  }
  return { name, cards, unrecognized }
}
