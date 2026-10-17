import type { CSSProperties } from "react"
import { useTranslation } from "react-i18next"

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
import type { NameSource } from "../../domain/nameDisplay"
import {
  type RarityBadgeAsset,
  type RarityBadgeInput,
  selectRarityBadges,
} from "../../domain/rarityBadges"
import { RARITY_BADGE_HEIGHT } from "./rarityBadgeSize"
import { useTextLanguage } from "./text-language"

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

export function RarityBadges(props: RarityBadgeInput & { readonly rarity?: NameSource }) {
  const { t } = useTranslation()
  const language = useTextLanguage()
  const badges = selectRarityBadges(props)
  const textPr =
    props.rarityCode?.toLowerCase() === "pr" &&
    props.rarityBadge?.style !== "none" &&
    badges.length === 0
  const vocabulary =
    Object.entries(props.rarity?.translations ?? {}).find(([lang]) => lang === language)?.[1] ||
    props.rarity?.original.text
  const rarity =
    vocabulary ||
    (props.rarityCode && props.rarityCode !== "-"
      ? props.rarityCode.toUpperCase()
      : t("rarityBadge.noRarity"))
  const official = t("rarityBadge.official", { rarity })
  const size = { "--rarity-badge-height": `${String(RARITY_BADGE_HEIGHT)}px` } as CSSProperties
  if (badges.length === 0 && !textPr) {
    return props.rarityBadge?.style === "none" && props.rarityCode !== "-" ? (
      <span
        role="img"
        aria-label={official}
        title={official}
        style={size}
        className="inline-block h-[min(var(--rarity-badge-height),1em)] w-[min(var(--rarity-badge-height),1em)] align-middle"
      />
    ) : null
  }
  return (
    <span
      data-rarity-badges=""
      style={size}
      className="inline-flex shrink-0 items-center gap-1 align-middle"
    >
      {textPr && (
        <span
          role="img"
          aria-label={official}
          title={official}
          className="inline-flex h-[1em] items-center text-[length:min(var(--rarity-badge-height),1em)] leading-none font-normal"
        >
          PR
        </span>
      )}
      {badges.map((badge) => (
        <img
          key={badge}
          src={ASSETS[badge]}
          alt={official}
          aria-label={official}
          title={official}
          width={56}
          height={21}
          className="h-[min(var(--rarity-badge-height),1em)] w-auto shrink-0"
        />
      ))}
    </span>
  )
}
