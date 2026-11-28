import { PROMO_RARITY } from "./representative"

/** The family code of the PR pack. */
export const PR_PACK = "pr"

/**
 * The official site lists PR pack rarities inconsistently (tokens as PR, leaders and EP as "-"),
 * so a PR pack printing is rated only by the badge printed at its lower right. The snapshot keeps
 * the official value; this decides what the app files and shows.
 */
export function adoptPrPackRarity<T extends Readonly<Record<string, unknown>>>(
  row: T,
  familyCode: string | undefined,
): T {
  // Locator rows carry no classification yet and stay pending.
  if (familyCode !== PR_PACK || !Object.hasOwn(row, "rarity_code")) return row
  const badge = row["rarity_badge"] as { readonly style?: unknown } | null | undefined
  return badge && badge.style !== "none"
    ? { ...row, rarity_code: PROMO_RARITY, premium: false }
    : { ...row, rarity_code: null, rarity_raw: "-", premium: null, rarity_badge: null }
}
