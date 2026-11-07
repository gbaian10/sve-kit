import { useMemo } from "react"

import { type DeckCodeCard, encodeDeckCode } from "../../domain/deck/code"
import { readImport } from "../../domain/deck/import"
import type { DeckEntry } from "../../domain/deck/types"
import type { Region } from "../../domain/search"
import { useDeckData } from "../decks/useDeckData"

export interface ScannedDeckShare {
  /** The list as the import page reads it ("count card-number name" lines). */
  readonly text: string
  /** A share code, or "" until the edition's printings have loaded or while cards are missing. */
  readonly code: string
  /** Card numbers the current card data cannot place; a code without them would be a smaller deck. */
  readonly missing: readonly string[]
}

/**
 * The scanned list and its share code. The code is made by reading the list exactly as the import
 * page would, so opening it there gives back the same deck.
 */
export function useScannedDeckShare(text: string, edition: Region, name: string): ScannedDeckShare {
  const data = useDeckData(edition)
  const facts = data.printingsReady ? data.facts : null
  return useMemo(() => {
    if (!facts || !text) return { text, code: "", missing: [] }
    const deck = readImport(text, {
      printingByCardNo: (cardNo) => facts.printingByCardNo(cardNo),
      printingByIntId: (intId) => facts.printingByIntId(intId),
      infoOf: (cardId) => facts.factsOf(cardId),
    })
    const missing = [
      ...deck.unknown.map((entry) => entry.label),
      ...deck.unrecognized.map((entry) => entry.text),
    ]
    const cards = (entries: readonly DeckEntry[]): DeckCodeCard[] =>
      entries.flatMap((entry) => {
        const intId = facts.intIdOf(entry)
        if (intId === undefined) missing.push(facts.factsOf(entry.cardId)?.name ?? entry.cardId)
        return intId === undefined ? [] : [{ intId, count: entry.count }]
      })
    const payload = {
      edition,
      format: deck.format ?? "standard",
      basis: deck.basis,
      leader: deck.leader ? (facts.intIdOf(deck.leader) ?? null) : null,
      main: cards(deck.main),
      evolve: cards(deck.evolve),
      side: cards(deck.side),
      name: deck.name ?? name,
    }
    // Only a code that carries every card; otherwise sharing falls back to the full text list.
    return { text, code: missing.length === 0 ? encodeDeckCode(payload) : "", missing }
  }, [facts, text, edition, name])
}
