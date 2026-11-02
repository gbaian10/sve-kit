import type { DeckCardInfo, DeckEntry } from "./types"

/** Cost slots of the curve: 0–7 and 8 or more. */
export const CURVE_SLOTS = ["0", "1", "2", "3", "4", "5", "6", "7", "8+"] as const

/** Copies per cost slot; cards without a cost are left out. */
export function costCurve(
  entries: readonly DeckEntry[],
  costOf: (cardId: string) => number | null,
): number[] {
  const curve = CURVE_SLOTS.map(() => 0)
  for (const entry of entries) {
    const cost = costOf(entry.cardId)
    if (cost === null) continue
    const slot = Math.min(CURVE_SLOTS.length - 1, Math.max(0, cost))
    curve[slot] = (curve[slot] ?? 0) + entry.count
  }
  return curve
}

/** Mean cost over the copies that have one; null when none does. */
export function averageCost(
  entries: readonly DeckEntry[],
  costOf: (cardId: string) => number | null,
): number | null {
  let copies = 0
  let sum = 0
  for (const entry of entries) {
    const cost = costOf(entry.cardId)
    if (cost === null) continue
    copies += entry.count
    sum += cost * entry.count
  }
  return copies === 0 ? null : sum / copies
}

export const TYPE_ORDER = ["follower", "spell", "amulet"] as const
export type CountedType = (typeof TYPE_ORDER)[number] | "other"

const countedType = (code: string | null | undefined): CountedType =>
  code === "follower" || code === "spell" || code === "amulet" ? code : "other"

export function typeCounts(
  entries: readonly DeckEntry[],
  infoOf: (cardId: string) => DeckCardInfo | undefined,
): Record<CountedType, number> {
  const counts: Record<CountedType, number> = { follower: 0, spell: 0, amulet: 0, other: 0 }
  for (const entry of entries) counts[countedType(infoOf(entry.cardId)?.typeCode)] += entry.count
  return counts
}

export type GroupMode = "type" | "cost"

export interface EntryGroup {
  /** A counted type for `type` groups; a curve slot, or null for no cost, for `cost` groups. */
  readonly key: CountedType | (typeof CURVE_SLOTS)[number] | null
  readonly count: number
  readonly entries: readonly DeckEntry[]
}

const typeRank = (type: CountedType): number =>
  type === "other" ? TYPE_ORDER.length : TYPE_ORDER.indexOf(type)

/** Entries in printed deck-list order (type when grouping by type, then cost, then name), grouped. */
export function groupEntries(
  entries: readonly DeckEntry[],
  infoOf: (cardId: string) => DeckCardInfo | undefined,
  mode: GroupMode,
): EntryGroup[] {
  const sorted = entries.toSorted((a, b) => {
    const ia = infoOf(a.cardId)
    const ib = infoOf(b.cardId)
    if (mode === "type") {
      const byType = typeRank(countedType(ia?.typeCode)) - typeRank(countedType(ib?.typeCode))
      if (byType !== 0) return byType
    }
    const byCost = (ia?.cost ?? Infinity) - (ib?.cost ?? Infinity)
    if (byCost !== 0 && !Number.isNaN(byCost)) return byCost
    return (ia?.name ?? a.cardId).localeCompare(ib?.name ?? b.cardId, "ja")
  })
  const groups = new Map<EntryGroup["key"], { count: number; entries: DeckEntry[] }>()
  for (const entry of sorted) {
    const info = infoOf(entry.cardId)
    const cost = info?.cost ?? null
    const key: EntryGroup["key"] =
      mode === "type"
        ? countedType(info?.typeCode)
        : cost === null
          ? null
          : (CURVE_SLOTS[Math.min(CURVE_SLOTS.length - 1, Math.max(0, cost))] ?? null)
    const group = groups.get(key) ?? { count: 0, entries: [] }
    group.entries.push(entry)
    group.count += entry.count
    groups.set(key, group)
  }
  return [...groups].map(([key, group]) => ({ key, ...group }))
}
