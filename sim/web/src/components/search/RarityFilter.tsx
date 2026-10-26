import { useId } from "react"
import { useTranslation } from "react-i18next"

import {
  orderRarities,
  orderRarityTokens,
  PREMIUM_RARITY,
  prStyleTokens,
  UNRATED_RARITY,
} from "../../domain/rarityFilter"
import type { Region } from "../../domain/search"
import { Switch } from "../ui/Switch"
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
  const id = useId()
  const visual = (content: FacetChipOption["content"]): Pick<FacetChipOption, "content"> =>
    content === undefined ? {} : { content }
  const children: FacetChipOption[] = prStyleTokens(options?.styles ?? []).map((code) => {
    const style = code.slice("pr:".length)
    return { code, label: prStyleLabel(t, style), ...visual(prStyleVisual(style, edition)) }
  })
  const items: FacetChip[] = orderRarities(rarities).map((rarity) =>
    rarity.code === "pr"
      ? { ...rarity, content: prBadgeVisual(), children }
      : { ...rarity, ...visual(rarityBadgeVisual({ rarityCode: rarity.code, region: edition })) },
  )
  items.push({
    code: UNRATED_RARITY,
    label: t("rarityBadge.unrated"),
    hint: t("rarityBadge.unratedHint"),
  })
  const premiumSelected = selected.includes(PREMIUM_RARITY)
  return (
    <div className="space-y-3">
      <FacetChips
        title={t("rarityBadge.label")}
        items={items}
        counts={options?.counts}
        selected={selected}
        onChange={onChange}
      />
      <div className="flex items-center gap-2">
        <Switch
          size="sm"
          checked={premiumSelected}
          disabled={!options?.counts}
          label={t("rarityBadge.premium")}
          describedBy={`${id}-premium-hint`}
          onChange={(checked) => {
            const next = selected.filter((token) => token !== PREMIUM_RARITY)
            onChange(orderRarityTokens(checked ? [...next, PREMIUM_RARITY] : next))
          }}
        />
        <div>
          <div className="text-13 text-text-1">{t("rarityBadge.premium")}</div>
          <p id={`${id}-premium-hint`} className="text-12 text-text-3">
            {t("rarityBadge.premiumHint")}
          </p>
        </div>
      </div>
    </div>
  )
}
