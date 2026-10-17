import br from "../../assets/rarity/br.svg"
import br_en from "../../assets/rarity/br-en.svg"
import gr from "../../assets/rarity/gr.svg"
import gr_en from "../../assets/rarity/gr-en.svg"
import lg from "../../assets/rarity/lg.svg"
import lg_en from "../../assets/rarity/lg-en.svg"
import pr_bronze from "../../assets/rarity/pr-bronze.svg"
import pr_gold from "../../assets/rarity/pr-gold.svg"
import pr_pastel from "../../assets/rarity/pr-pastel.svg"
import pr_rainbow from "../../assets/rarity/pr-rainbow.svg"
import pr_red from "../../assets/rarity/pr-red.svg"
import pr_silver from "../../assets/rarity/pr-silver.svg"
import sl from "../../assets/rarity/sl.svg"
import sp from "../../assets/rarity/sp.svg"
import sr from "../../assets/rarity/sr.svg"
import sr_en from "../../assets/rarity/sr-en.svg"
import ssp from "../../assets/rarity/ssp.svg"
import ur from "../../assets/rarity/ur.svg"
import ur_en from "../../assets/rarity/ur-en.svg"
import type { RarityBadgeAsset } from "../../domain/rarityBadges"

const ASSETS: Readonly<Record<RarityBadgeAsset, string>> = {
  "br-en": br_en,
  br: br,
  "gr-en": gr_en,
  gr: gr,
  "lg-en": lg_en,
  lg: lg,
  "pr-bronze": pr_bronze,
  "pr-gold": pr_gold,
  "pr-pastel": pr_pastel,
  "pr-rainbow": pr_rainbow,
  "pr-red": pr_red,
  "pr-silver": pr_silver,
  sl: sl,
  sp: sp,
  "sr-en": sr_en,
  sr: sr,
  ssp: ssp,
  "ur-en": ur_en,
  ur: ur,
}

/** The drawn badge for an asset name from `selectRarityBadges`. */
export function rarityBadgeSrc(asset: RarityBadgeAsset): string {
  return ASSETS[asset]
}
