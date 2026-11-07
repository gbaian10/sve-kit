import { useMemo } from "react"

import { type DeckCodeCard, encodeDeckCode } from "../../domain/deck/code"
import { readImport } from "../../domain/deck/import"
import type { DeckEntry } from "../../domain/deck/types"
import type { Region } from "../../domain/search"
import { useDeckData } from "../decks/useDeckData"

export interface ScannedDeckShare {
  /** The list as the import page reads it ("count card-number name" lines). */
  readonly text: string
  /** A share code, or "" until the edition's printings have loaded. */
  readonly code: string
}

/**
 * The scanned list and its share code. The code is made by reading the list exactly as the import
 * page would, so opening it there gives back the same deck.
 */
export function useScannedDeckShare(text: string, edition: Region, name: string): ScannedDeckShare {
  const data = useDeckData(edition)
  const facts = data.printingsReady ? data.facts : null
  const code = useMemo(() => {
    if (!facts || !text) return ""
    const deck = readImport(text, {
      printingByCardNo: (cardNo) => facts.printingByCardNo(cardNo),
      printingByIntId: (intId) => facts.printingByIntId(intId),
      infoOf: (cardId) => facts.factsOf(cardId),
    })
    const cards = (entries: readonly DeckEntry[]): DeckCodeCard[] =>
      entries.flatMap((entry) => {
        const intId = facts.intIdOf(entry)
        return intId === undefined ? [] : [{ intId, count: entry.count }]
      })
    return encodeDeckCode({
      edition,
      format: deck.format ?? "standard",
      basis: deck.basis,
      leader: deck.leader ? (facts.intIdOf(deck.leader) ?? null) : null,
      main: cards(deck.main),
      evolve: cards(deck.evolve),
      side: cards(deck.side),
      name: deck.name ?? name,
    })
  }, [facts, text, edition, name])
  return { text, code }
}
