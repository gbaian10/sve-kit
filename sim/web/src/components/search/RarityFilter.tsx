import { useTranslation } from "react-i18next"

import {
  orderRarities,
  PREMIUM_RARITY,
  prStyleTokens,
  UNRATED_RARITY,
} from "../../domain/rarityFilter"
import type { Region } from "../../domain/search"
import { type FacetChip, type FacetChipOption, FacetChips } from "./FacetChips"
import { prBadgeVisual, prStyleLabel, prStyleVisual, rarityBadgeVisual } from "./rarityVisuals"

// Chips show the badge drawn for the edition's card face (the maintainer's choice); PR uses the
// grey drawing made for the filter, its appearances sit behind the popover, "unrated" stays text.
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
  const visual = (content: FacetChipOption["content"]): Pick<FacetChipOption, "content"> =>
    content === undefined ? {} : { content }
  const children: FacetChipOption[] = prStyleTokens(options?.styles ?? []).map((code) => {
    const style = code.slice("pr:".length)
    return { code, label: prStyleLabel(t, style), ...visual(prStyleVisual(style, edition)) }
  })
  const items: FacetChip[] = orderRarities([
    ...rarities,
    { code: PREMIUM_RARITY, label: t("rarityBadge.premium") },
  ]).map((rarity) =>
    rarity.code === "pr"
      ? { ...rarity, content: prBadgeVisual(), children }
      : rarity.code === PREMIUM_RARITY
        ? rarity
        : { ...rarity, ...visual(rarityBadgeVisual({ rarityCode: rarity.code, region: edition })) },
  )
  items.push({
    code: UNRATED_RARITY,
    label: t("rarityBadge.unrated"),
    hint: t("rarityBadge.unratedHint"),
  })
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
