import { useTranslation } from "react-i18next"

import pr from "../../assets/rarity/pr.svg"
import { BADGE_STYLES, type RarityBadgeInput, selectRarityBadges } from "../../domain/rarityBadges"
import { orderRarities, UNRATED_RARITY } from "../../domain/rarityFilter"
import type { Region } from "../../domain/search"
import { rarityBadgeSrc } from "../card/rarityBadgeAssets"
import { type FacetChip, type FacetChipOption, FacetChips } from "./FacetChips"

function Badge({ src }: { readonly src: string }) {
  return <img src={src} alt="" width={56} height={21} className="h-4 w-auto" />
}

// Chips show the badge drawn for the edition's card face (the maintainer's choice). PR as a whole
// has no printed badge: the chip uses the grey drawing made for it, which card faces never show;
// its appearances sit behind the popover. "Unrated" stays text.
export function RarityFilter({
  rarities,
  options,
  selected,
  onChange,
  edition,
}: {
  readonly rarities: readonly { readonly code: string; readonly label: string }[]
  readonly options:
    | { readonly styles: readonly string[]; readonly counts: Readonly<Record<string, number>> }
    | undefined
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
  readonly edition: Region
}) {
  const { t } = useTranslation()
  const badge = (input: RarityBadgeInput): Pick<FacetChipOption, "content"> => {
    const [asset] = selectRarityBadges(input)
    return asset ? { content: <Badge src={rarityBadgeSrc(asset)} /> } : {}
  }
  const children: FacetChipOption[] = ([...BADGE_STYLES, "unknown"] as const)
    .filter((style) => options?.styles.includes(style))
    .map((style) =>
      style === "none" || style === "unknown"
        ? {
            code: `pr:${style}`,
            label: t("rarityBadge.prStyle", { style: t(`rarityBadge.${style}`) }),
          }
        : {
            code: `pr:${style}`,
            label: t("rarityBadge.prStyle", { style: style.toUpperCase() }),
            ...badge({ rarityCode: "pr", region: edition, rarityBadge: { style, text: "PR" } }),
          },
    )
  const items: FacetChip[] = [
    ...orderRarities(rarities).map((rarity) =>
      rarity.code === "pr"
        ? { ...rarity, content: <Badge src={pr} />, children }
        : { ...rarity, ...badge({ rarityCode: rarity.code, region: edition }) },
    ),
    { code: UNRATED_RARITY, label: t("rarityBadge.unrated") },
  ]
  return (
    <FacetChips
      title={t("rarityBadge.label")}
      items={items}
      counts={options?.counts}
      selected={selected}
      onChange={onChange}
    />
  )
}
