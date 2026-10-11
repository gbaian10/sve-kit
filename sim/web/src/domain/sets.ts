import { compareCodePoints } from "./normalize"
import type { Region } from "./search"

// The home page's set entries (#519). Product release dates only order sets and are shown; they
// never decide which printings a set holds (`set=` stays on the printing's home set).
export interface SetFacts {
  readonly id: string
  readonly publicCode: string
  readonly kind: string
}

export interface ProductFacts {
  /** The set this product belongs to; null when the link is not recorded. */
  readonly familyId: string | null
  readonly region: string
  /** ISO day, or null when the product has no day-precision date. */
  readonly releasedOn: string | null
}

export interface DatedSet {
  readonly id: string
  readonly publicCode: string
  /** The first release of the set in the region. */
  readonly releasedOn: string
}

/**
 * The newest booster sets of a region, newest first. A set counts only through a product that
 * links to it and has a known day; a set without one is left out rather than dated by its code.
 */
export function recentSets(
  sets: readonly SetFacts[],
  products: readonly ProductFacts[],
  region: Region,
  limit: number,
): DatedSet[] {
  const firstRelease = new Map<string, string>()
  for (const product of products) {
    if (product.region !== region || product.familyId === null || product.releasedOn === null)
      continue
    const known = firstRelease.get(product.familyId)
    if (known === undefined || product.releasedOn < known)
      firstRelease.set(product.familyId, product.releasedOn)
  }
  return sets
    .filter((set) => set.kind === "booster")
    .flatMap((set) => {
      const releasedOn = firstRelease.get(set.id)
      return releasedOn === undefined
        ? []
        : [{ id: set.id, publicCode: set.publicCode, releasedOn }]
    })
    .sort(
      (a, b) =>
        compareCodePoints(b.releasedOn, a.releasedOn) ||
        compareCodePoints(b.publicCode, a.publicCode),
    )
    .slice(0, limit)
}

export interface PackCard {
  /** One card, whatever printing represents it. */
  readonly key: string
  /** Has a name translation in the counted language, low confidence included. */
  readonly translated: boolean
  readonly lowConfidence: boolean
}

export interface PackCounts {
  readonly cards: number
  readonly translated: number
  /** Translated cards whose name translation still awaits proofreading. */
  readonly proofreading: number
}

/** Counts each card once, so several printings of a card in the set never inflate the totals. */
export function packCounts(items: readonly PackCard[]): PackCounts {
  const cards = new Map<string, { translated: boolean; lowConfidence: boolean }>()
  for (const item of items) {
    const known = cards.get(item.key)
    cards.set(item.key, {
      translated: (known?.translated ?? false) || item.translated,
      lowConfidence: (known?.lowConfidence ?? false) || (item.translated && item.lowConfidence),
    })
  }
  let translated = 0
  let proofreading = 0
  for (const card of cards.values()) {
    if (!card.translated) continue
    translated += 1
    if (card.lowConfidence) proofreading += 1
  }
  return { cards: cards.size, translated, proofreading }
}
