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
  /** All sets this product belongs to; empty when no links are recorded. */
  readonly familyIds: readonly string[]
  readonly region: string
  /** ISO day, or null when the product has no day-precision date. */
  readonly releasedOn: string | null
}

/**
 * Each set with its first release in the region. A set counts only through a product that links
 * to it and has a known day; a set without one is left out rather than dated by its code.
 */
export function datedSets<T extends SetFacts>(
  sets: readonly T[],
  products: readonly ProductFacts[],
  region: Region,
): (T & { readonly releasedOn: string })[] {
  const firstRelease = new Map<string, string>()
  for (const product of products) {
    if (product.region !== region || product.releasedOn === null) continue
    for (const familyId of product.familyIds) {
      const known = firstRelease.get(familyId)
      if (known === undefined || product.releasedOn < known)
        firstRelease.set(familyId, product.releasedOn)
    }
  }
  return sets.flatMap((set) => {
    const releasedOn = firstRelease.get(set.id)
    return releasedOn === undefined ? [] : [{ ...set, releasedOn }]
  })
}

/** The newest booster and collaboration sets, newest first. */
export function recentSets<T extends SetFacts & { readonly releasedOn: string }>(
  sets: readonly T[],
  limit: number,
): T[] {
  return sets
    .filter((set) => set.kind === "booster" || set.kind === "collaboration")
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
