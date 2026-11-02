import {
  arrayValue,
  type CardSummary,
  type Catalog,
  faceBasics,
  nameOf,
  objectValue,
  type Row,
  stringValue,
} from "../../data"
import type { DeckCardInfo } from "../../domain/deck/types"
import type { NameSource } from "../../domain/nameDisplay"
import type { Region } from "../../domain/search"

interface PrintingFact {
  readonly id: string
  readonly cardNo: string
  readonly rarityCode: string | null
  readonly summary: CardSummary | undefined
}

/** A card as the deck builder sees it: the rule facts plus what its rows display. */
interface DeckCardFacts extends DeckCardInfo {
  readonly attack: number | null
  readonly defense: number | null
  readonly defaultPrintingId: string | undefined
  /** The default printing's summary (names, stats, badges) for rows and images. */
  readonly summary: CardSummary | undefined
  readonly nameSource: NameSource | undefined
  /** The edition's printings of the card, by card number. */
  readonly printings: readonly PrintingFact[]
}

export interface DeckFacts {
  readonly edition: Region
  readonly factsOf: (cardId: string) => DeckCardFacts | undefined
  /** The summary a deck entry shows: its chosen printing, else the card's default printing. */
  readonly summaryOf: (cardId: string, printingId?: string) => CardSummary | undefined
  /** Family codes of the sets whose cards carry a title, for the pool's legal-only filter. */
  readonly setsOfTitle: (titleCode: string) => readonly string[]
  /**
   * The cost an evolve card is charted at. Evolved cards print no cost, so the curve uses the cost
   * of the main-deck follower of the same name, the card it evolves from.
   */
  readonly chartCostOf: (cardId: string) => number | null
  /** The card number of an entry's printing (its chosen one, else the card's default). */
  readonly cardNoOf: (entry: {
    readonly cardId: string
    readonly printingId?: string
  }) => string | undefined
  /** The permanent printing integer a share code carries for an entry. */
  readonly intIdOf: (entry: {
    readonly cardId: string
    readonly printingId?: string
  }) => number | undefined
  /** A pasted card number (any case, the EN suffix optional) to a printing of the edition. */
  readonly printingByCardNo: (cardNo: string) => DeckPrinting | undefined
  /** A share code's printing integer to a printing of the edition. */
  readonly printingByIntId: (intId: number) => DeckPrinting | undefined
}

interface DeckPrinting {
  readonly cardId: string
  readonly printingId: string
}

const strings = (value: unknown): string[] =>
  Array.isArray(value)
    ? (value as unknown[]).filter((item): item is string => typeof item === "string")
    : []

/**
 * A pasted card number matched loosely: any case, with or without the EN mark, which EN numbers
 * carry after the set code or at the end. Card numbers themselves stay as printed.
 */
const cardNoKey = (cardNo: string) =>
  cardNo
    .trim()
    .toUpperCase()
    .replace(/EN(?=-)|EN$/u, "")

const numberOrNull = (value: unknown): number | null => (typeof value === "number" ? value : null)

export function createDeckFacts(catalog: Catalog, edition: Region): DeckFacts {
  const index = catalog.index
  const cache = new Map<string, DeckCardFacts | undefined>()
  const frontRevision = (cardId: string): Row | undefined => {
    const front = index.facesOf(cardId)[0]
    return front ? index.displayRevision(stringValue(front["id"]), edition) : undefined
  }
  const factsOf = (cardId: string): DeckCardFacts | undefined => {
    if (cache.has(cardId)) return cache.get(cardId)
    const card = index.card(cardId)
    const front = index.facesOf(cardId)[0]
    if (!card || !front) {
      cache.set(cardId, undefined)
      return undefined
    }
    const faceId = stringValue(front["id"])
    const region = arrayValue(card["regions"] ?? [])
      .map((value) => objectValue(value))
      .find((row) => row["region"] === edition)
    const printings: PrintingFact[] = index
      .printingsOf(cardId)
      .filter((row) => row["region"] === edition)
      .map((row) => ({
        id: stringValue(row["id"]),
        cardNo: stringValue(row["card_no"]),
        rarityCode: typeof row["rarity_code"] === "string" ? row["rarity_code"] : null,
        summary: catalog.summary(stringValue(row["id"])),
      }))
      .sort((a, b) => a.cardNo.localeCompare(b.cardNo, "en"))
    const defaultPrintingId =
      typeof region?.["default_printing_id"] === "string"
        ? region["default_printing_id"]
        : printings[0]?.id
    const summary = defaultPrintingId === undefined ? undefined : catalog.summary(defaultPrintingId)
    const revision =
      index.displayRevision(faceId, edition) ??
      (defaultPrintingId === undefined
        ? undefined
        : faceBasics(index, faceId, edition, defaultPrintingId).values)
    const nameSource = summary?.name ?? (revision ? nameOf(index, revision) : undefined)
    const facts: DeckCardFacts = {
      cardId,
      name: nameSource?.original.text ?? summary?.cardNo ?? cardId,
      classCode:
        summary?.classCode ??
        (typeof revision?.["class_code"] === "string" ? revision["class_code"] : null),
      typeCode:
        summary?.typeCode ??
        (typeof revision?.["type_code"] === "string" ? revision["type_code"] : null),
      specialKinds: strings(revision?.["special_kinds"]),
      titles: strings(revision?.["titles"]),
      cost: summary?.cost ?? numberOrNull(revision?.["cost"]),
      attack: summary?.attack ?? numberOrNull(revision?.["attack"]),
      defense: summary?.defense ?? numberOrNull(revision?.["defense"]),
      deckRole: typeof region?.["deck_role"] === "string" ? region["deck_role"] : null,
      defaultPrintingId,
      summary,
      nameSource,
      printings,
    }
    cache.set(cardId, facts)
    return facts
  }
  let titleSets: Map<string, Set<string>> | undefined
  const setsByTitle = () => {
    if (titleSets) return titleSets
    titleSets = new Map()
    for (const card of index.cards) {
      const cardId = stringValue(card["id"])
      const titles = strings(frontRevision(cardId)?.["titles"])
      if (titles.length === 0) continue
      const families = index
        .printingsOf(cardId)
        .filter((row) => row["region"] === edition)
        .flatMap((row) => {
          const family = index.family(stringValue(row["home_set_id"]))
          return family ? [stringValue(family["code"])] : []
        })
      for (const title of titles) {
        const sets = titleSets.get(title) ?? new Set<string>()
        for (const code of families) sets.add(code)
        titleSets.set(title, sets)
      }
    }
    return titleSets
  }
  let baseCosts: Map<string, number> | undefined
  const baseCostByName = () => {
    if (baseCosts) return baseCosts
    baseCosts = new Map()
    for (const card of index.cards) {
      const revision = frontRevision(stringValue(card["id"]))
      if (revision?.["type_code"] !== "follower" || typeof revision["cost"] !== "number") continue
      const kinds = strings(revision["special_kinds"])
      if (kinds.some((kind) => kind === "evolve" || kind === "advance" || kind === "token"))
        continue
      const unit = index.textUnit(stringValue(revision["name_unit_id"]))
      if (unit) baseCosts.set(stringValue(unit["text"]), revision["cost"])
    }
    return baseCosts
  }
  const summaryOf = (cardId: string, printingId?: string) => {
    const facts = factsOf(cardId)
    const chosen =
      printingId === undefined ? undefined : facts?.printings.find((item) => item.id === printingId)
    return chosen?.summary ?? facts?.summary
  }
  const printingOf = (row: Row | undefined): DeckPrinting | undefined =>
    row ? { cardId: stringValue(row["card_id"]), printingId: stringValue(row["id"]) } : undefined
  let byIntId: Map<number, DeckPrinting> | undefined
  let byCardNo: Map<string, DeckPrinting> | undefined
  const entryPrinting = (entry: { readonly cardId: string; readonly printingId?: string }) => {
    const id = entry.printingId ?? factsOf(entry.cardId)?.defaultPrintingId
    return id === undefined ? undefined : index.printing(id)
  }
  return {
    edition,
    factsOf,
    summaryOf,
    cardNoOf: (entry) => {
      const row = entryPrinting(entry)
      return row ? stringValue(row["card_no"]) : undefined
    },
    intIdOf: (entry) => {
      const value = entryPrinting(entry)?.["int_id"]
      // Reprints exist on the main thread only as locators; their summary still knows the integer.
      return typeof value === "number" ? value : summaryOf(entry.cardId, entry.printingId)?.intId
    },
    printingByCardNo: (cardNo) => {
      if (!byCardNo) {
        byCardNo = new Map()
        for (const card of index.cards)
          for (const row of index.printingsOf(stringValue(card["id"])))
            if (row["region"] === edition) {
              const key = cardNoKey(stringValue(row["card_no"]))
              const hit = printingOf(row)
              if (hit && !byCardNo.has(key)) byCardNo.set(key, hit)
            }
      }
      return byCardNo.get(cardNoKey(cardNo))
    },
    printingByIntId: (intId) => {
      if (!byIntId) {
        byIntId = new Map()
        for (const card of index.cards)
          for (const row of index.printingsOf(stringValue(card["id"])))
            if (row["region"] === edition && typeof row["int_id"] === "number")
              byIntId.set(row["int_id"], {
                cardId: stringValue(row["card_id"]),
                printingId: stringValue(row["id"]),
              })
      }
      return byIntId.get(intId)
    },
    setsOfTitle: (titleCode) => [...(setsByTitle().get(titleCode) ?? [])],
    chartCostOf: (cardId) => {
      const facts = factsOf(cardId)
      if (!facts) return null
      if (facts.cost !== null) return facts.cost
      if (!facts.specialKinds.includes("evolve")) return null
      return baseCostByName().get(facts.name) ?? null
    },
  }
}
