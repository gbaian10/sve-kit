import type { SearchOption } from "./index"

export function uniquePromoLeader(
  promoSets: readonly SearchOption[],
  counts: Readonly<Record<string, number>>,
): SearchOption | undefined {
  let leader: SearchOption | undefined
  let highest = -1
  let tied = false
  for (const set of promoSets) {
    const count = counts[set.code] ?? 0
    if (count > highest) {
      leader = set
      highest = count
      tied = false
    } else if (count === highest) {
      tied = true
    }
  }
  return tied ? undefined : leader
}
