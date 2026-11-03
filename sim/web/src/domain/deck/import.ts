import { NEUTRAL_CLASS } from "../query/model"
import type { Region } from "../search"
import { deckCodeOf, decodeDeckCode } from "./code"
import { placementOf } from "./model"
import { parseDeckText } from "./text"
import type { DeckBasis, DeckCardInfo, DeckEntry, DeckFormat, DeckLeader } from "./types"

export interface ImportLookup {
  /** A card number as printed (any case; the EN suffix optional) to a printing of the edition. */
  readonly printingByCardNo: (cardNo: string) => ImportedPrinting | undefined
  /** A share code's permanent printing integer to a printing of the edition. */
  readonly printingByIntId: (intId: number) => ImportedPrinting | undefined
  readonly infoOf: (
    cardId: string,
  ) => (DeckCardInfo & { readonly defaultPrintingId?: string | undefined }) | undefined
}

interface ImportedPrinting {
  readonly cardId: string
  readonly printingId: string
}

export interface ImportedDeck {
  readonly name: string | undefined
  /** Set by a share code; a pasted list leaves the edition to the importer. */
  readonly edition: Region | undefined
  /** Set by a share code; a pasted list names no format, so the deck becomes Standard. */
  readonly format: DeckFormat | undefined
  readonly basis: DeckBasis
  readonly leader: DeckLeader | null
  readonly main: readonly DeckEntry[]
  readonly evolve: readonly DeckEntry[]
  readonly side: readonly DeckEntry[]
  /** Cards the snapshot has no printing for: card numbers, or `#<int id>` from a code. */
  readonly unknown: readonly { readonly label: string; readonly count: number }[]
  readonly unrecognized: readonly { readonly line: number; readonly text: string }[]
}

interface Reference {
  readonly label: string
  readonly printing: ImportedPrinting | undefined
  readonly count: number
  readonly side: boolean
  readonly leader: boolean
}

/** Which edition a pasted text needs: a share code says so; a list does not. */
export function importEdition(text: string): Region | undefined {
  const code = deckCodeOf(text)
  return code === undefined ? undefined : decodeDeckCode(code).edition
}

/**
 * Reads a pasted list or share code against one edition's cards. Each card goes where it belongs
 * (leader, main or evolve deck); only the side list follows the text. Throws `DeckCodeError` for a
 * malformed code.
 */
export function readImport(text: string, lookup: ImportLookup): ImportedDeck {
  const code = deckCodeOf(text)
  const references: Reference[] = []
  let name: string | undefined
  let edition: Region | undefined
  let format: DeckFormat | undefined
  let basis: DeckBasis | undefined
  let unrecognized: ImportedDeck["unrecognized"] = []
  if (code === undefined) {
    const parsed = parseDeckText(text)
    name = parsed.name
    unrecognized = parsed.unrecognized
    for (const card of parsed.cards)
      references.push({
        label: card.cardNo,
        printing: lookup.printingByCardNo(card.cardNo),
        count: card.count,
        side: card.side,
        leader: false,
      })
  } else {
    const payload = decodeDeckCode(code)
    name = payload.name || undefined
    edition = payload.edition
    format = payload.format
    basis = payload.basis
    const add = (intId: number, count: number, side: boolean, leader: boolean) => {
      references.push({
        label: `#${String(intId)}`,
        printing: lookup.printingByIntId(intId),
        count,
        side,
        leader,
      })
    }
    if (payload.leader !== null) add(payload.leader, 1, false, true)
    for (const card of [...payload.main, ...payload.evolve])
      add(card.intId, card.count, false, false)
    for (const card of payload.side) add(card.intId, card.count, true, false)
  }

  const parts = {
    main: new Map<string, DeckEntry>(),
    evolve: new Map<string, DeckEntry>(),
    side: new Map<string, DeckEntry>(),
  }
  const unknown: { label: string; count: number }[] = []
  let leader: DeckLeader | null = null
  for (const reference of references) {
    const info = reference.printing && lookup.infoOf(reference.printing.cardId)
    if (!reference.printing || !info) {
      unknown.push({ label: reference.label, count: reference.count })
      continue
    }
    // The printing named is kept only when it is not the card's default one.
    const chosen =
      info.defaultPrintingId === reference.printing.printingId
        ? {}
        : { printingId: reference.printing.printingId }
    const placement = placementOf(info)
    if (placement === "leader" && !reference.side) {
      leader ??= { cardId: info.cardId, ...chosen }
      continue
    }
    const part = reference.side ? parts.side : placement === "evolve" ? parts.evolve : parts.main
    const existing = part.get(info.cardId)
    part.set(info.cardId, {
      cardId: info.cardId,
      count: (existing?.count ?? 0) + reference.count,
      ...(existing?.printingId === undefined ? chosen : { printingId: existing.printingId }),
    })
  }
  const main = [...parts.main.values()]
  const evolve = [...parts.evolve.values()]
  return {
    name,
    edition,
    format,
    basis: basis ?? inferBasis(leader, [...main, ...evolve], lookup),
    leader,
    main,
    evolve,
    side: [...parts.side.values()],
    unknown,
    unrecognized,
  }
}

/** A list does not say its basis: the leader's class, else the class most of its cards have. */
function inferBasis(
  leader: DeckLeader | null,
  entries: readonly DeckEntry[],
  lookup: ImportLookup,
): DeckBasis {
  const leaderClass = leader ? lookup.infoOf(leader.cardId)?.classCode : null
  if (leaderClass) return { kind: "class", classCode: leaderClass }
  const copies = new Map<string, number>()
  for (const entry of entries) {
    const classCode = lookup.infoOf(entry.cardId)?.classCode
    if (classCode && classCode !== NEUTRAL_CLASS)
      copies.set(classCode, (copies.get(classCode) ?? 0) + entry.count)
  }
  const [top] = [...copies].sort((a, b) => b[1] - a[1])
  return { kind: "class", classCode: top?.[0] ?? NEUTRAL_CLASS }
}
