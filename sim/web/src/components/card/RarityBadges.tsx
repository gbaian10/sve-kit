import type { CSSProperties } from "react"
import { useTranslation } from "react-i18next"

import type { NameSource } from "../../domain/nameDisplay"
import { type RarityBadgeInput, selectRarityBadges } from "../../domain/rarityBadges"
import { rarityBadgeSrc } from "./rarityBadgeAssets"
import { RARITY_BADGE_HEIGHT } from "./rarityBadgeSize"
import { useTextLanguage } from "./text-language"

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
          src={rarityBadgeSrc(badge)}
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
