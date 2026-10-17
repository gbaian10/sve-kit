import { useTranslation } from "react-i18next"

import { BADGE_STYLES } from "../../domain/rarityBadges"
import { UNRATED_RARITY } from "../../domain/rarityFilter"
import { FacetFilter } from "./FacetFilter"

export function RarityFilter({
  rarities,
  options,
  selected,
  onChange,
}: {
  readonly rarities: readonly { readonly code: string; readonly label: string }[]
  readonly options:
    | { readonly styles: readonly string[]; readonly counts: Readonly<Record<string, number>> }
    | undefined
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const children = ([...BADGE_STYLES, "unknown"] as const)
    .filter((style) => options?.styles.includes(style))
    .map((style) => ({
      code: `pr:${style}`,
      label: t("rarityBadge.prStyle", {
        style:
          style === "none" || style === "unknown" ? t(`rarityBadge.${style}`) : style.toUpperCase(),
      }),
    }))
  return (
    <FacetFilter
      label={t("rarityBadge.label")}
      groups={[
        [
          ...rarities.map((rarity) =>
            rarity.code === "pr" ? { ...rarity, children, fullWidth: true } : rarity,
          ),
          { code: UNRATED_RARITY, label: t("rarityBadge.unrated") },
        ],
      ]}
      counts={options?.counts}
      selected={selected}
      onChange={onChange}
    />
  )
}
