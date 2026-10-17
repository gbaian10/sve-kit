import type { TFunction } from "i18next"
import type { ReactNode } from "react"

import pr from "../../assets/rarity/pr.svg"
import { BADGE_STYLES, type RarityBadgeInput, selectRarityBadges } from "../../domain/rarityBadges"
import { rarityBadgeSrc } from "../card/rarityBadgeAssets"

const image = (src: string, alt: string) => (
  <img src={src} alt={alt} width={56} height={21} className="h-4 w-auto" />
)

/** The badge drawn for a rarity on the edition's card face, if there is one. */
export function rarityBadgeVisual(input: RarityBadgeInput, alt = ""): ReactNode | undefined {
  const [asset] = selectRarityBadges(input)
  return asset ? image(rarityBadgeSrc(asset), alt) : undefined
}

// PR as a whole has no printed badge: the filter uses the grey drawing made for it, which card
// faces never show.
export function prBadgeVisual(alt = ""): ReactNode {
  return image(pr, alt)
}

/** The badge of one PR appearance style (`pr:<style>`); none for "none" and "unknown". */
export function prStyleVisual(style: string, region: RarityBadgeInput["region"], alt = "") {
  const known = BADGE_STYLES.find((candidate) => candidate === style && candidate !== "none")
  return known
    ? rarityBadgeVisual(
        { rarityCode: "pr", region, rarityBadge: { style: known, text: "PR" } },
        alt,
      )
    : undefined
}

export function prStyleLabel(t: TFunction, style: string): string {
  return t("rarityBadge.prStyle", {
    style:
      style === "none" || style === "unknown" ? t(`rarityBadge.${style}`) : style.toUpperCase(),
  })
}
