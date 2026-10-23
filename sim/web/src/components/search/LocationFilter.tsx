import { useTranslation } from "react-i18next"

import amulet from "../../assets/card-labels/amulet.svg"
import crest from "../../assets/card-labels/crest.svg"
import equipment from "../../assets/card-labels/equipment.svg"
import follower from "../../assets/card-labels/follower.svg"
import mainDeck from "../../assets/card-labels/main-deck.svg"
import spell from "../../assets/card-labels/spell.svg"
import type { SearchOptions } from "../../data"
import { leaderFrameTokens, LOCATION_ORDER, tokenKindTokens } from "../../domain/deckLocation"
import type { TextLang } from "../../domain/search"
import { DeckLocationBadges } from "../card/DeckLocationBadges"
import { locationLabel, vocabularyOptionLabel } from "./facet-labels"
import { type FacetChip, FacetChips } from "./FacetChips"

// Card-type plates for the Token kinds in this filter's popover only.
const TOKEN_KIND_PLATES: Readonly<Record<string, string>> = {
  follower,
  spell,
  amulet,
  crest,
  equipment,
}

// Where a printing goes in a deck; a chip's sub-values (Token kinds, leader frames) sit behind its
// popover like the former type filter's did, so their tokens and URLs are unchanged.
export function LocationFilter({
  options,
  textLang,
  selected,
  onChange,
}: {
  readonly options:
    | { readonly types: SearchOptions["types"]; readonly counts: SearchOptions["locationCounts"] }
    | undefined
  readonly textLang: TextLang
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const counts = options?.counts ?? {}
  const typeLabel = (code: string) => {
    const type = options?.types.find((item) => item.code === code)
    return type ? vocabularyOptionLabel(type.name, textLang) : code
  }
  const items = LOCATION_ORDER.flatMap((code): FacetChip[] => {
    // Rows the edition has no printings for stay out, as the type row does; "main" always shows.
    if (code !== "main" && !(counts[code] ?? 0) && options) return []
    const label = locationLabel(t, code)
    // The main deck shows a mini card back with its name (the most used and least obvious choice);
    // the other positions show their card-face plates.
    const content =
      code === "main" ? (
        <span className="inline-flex items-center gap-1.5">
          <img src={mainDeck} alt="" aria-hidden className="h-6 w-auto shrink-0" />
          <span>{label}</span>
        </span>
      ) : (
        <DeckLocationBadges locations={[code]} decorative />
      )
    if (code === "token")
      return [
        {
          code,
          label,
          content,
          // Each Token kind shows its card-type plate beside the name.
          children: tokenKindTokens(counts).map((token) => {
            const kind = token.slice("token:".length)
            const kindLabel = typeLabel(kind)
            const plate = TOKEN_KIND_PLATES[kind]
            return {
              code: token,
              label: kindLabel,
              content: plate ? (
                <span className="inline-flex items-center gap-2">
                  <img src={plate} alt="" aria-hidden className="h-5 w-auto shrink-0" />
                  <span>{kindLabel}</span>
                </span>
              ) : undefined,
            }
          }),
        },
      ]
    if (code === "leader")
      return [
        {
          code,
          label,
          content,
          children: leaderFrameTokens(counts).map((token) => ({
            code: token,
            label: locationLabel(t, token),
          })),
        },
      ]
    return [{ code, label, content }]
  })
  return (
    <FacetChips
      title={t("locationFilter.label")}
      locationGrid
      items={items}
      counts={options?.counts}
      selected={selected}
      onChange={onChange}
    />
  )
}
