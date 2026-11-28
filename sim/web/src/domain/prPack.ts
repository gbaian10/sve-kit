import type { Row } from "../data"
import type { RarityBadge } from "./rarityBadges"
import { PROMO_RARITY } from "./representative"

/** The family code of the PR pack. */
export const PR_PACK = "pr"

/** The printed badge resolves inconsistencies in the official PR pack rarity listings. */
export function adoptPrPackRarity(row: Readonly<Row>, familyCode: string | undefined): Row {
  // Locator rows carry no classification yet and stay pending.
  if (familyCode !== PR_PACK || !Object.hasOwn(row, "rarity_code")) return row
  const style = (row["rarity_badge"] as RarityBadge | null | undefined)?.style
  return style && style !== "none"
    ? { ...row, rarity_code: PROMO_RARITY, premium: false }
    : { ...row, rarity_code: null, rarity_raw: "-", premium: null, rarity_badge: null }
}
