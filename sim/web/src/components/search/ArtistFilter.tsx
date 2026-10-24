import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import {
  artistChecked,
  CREDITED_ARTIST,
  creditedState,
  toggleArtist,
  toggleCredited,
  UNCREDITED_ARTIST,
} from "../../domain/artistFilter"
import type { TextLang } from "../../domain/search"
import { FacetCheckbox, FacetFilter } from "./FacetFilter"

// Artists are credited as the official site prints them, so there is nothing to translate: the
// list is the printed names, ordered in the UI language's collation, with a search box. Above it
// two summary rows cover the whole edition: every credited printing (ticking selects all artists)
// and the printings nobody credits.
export function ArtistFilter({
  options,
  textLang,
  selected,
  onChange,
}: {
  readonly options:
    | { readonly artists: SearchOptions["artists"]; readonly counts: SearchOptions["artistCounts"] }
    | undefined
  readonly textLang: TextLang
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const collator = new Intl.Collator(textLang === "zh-Hant" ? "zh-Hant" : textLang)
  const items = (options?.artists ?? [])
    .map((artist) => ({ code: artist.id, label: artist.name }))
    .sort((a, b) => collator.compare(a.label, b.label) || a.code.localeCompare(b.code))
  const allIds = items.map((item) => item.code)
  const counts = options?.counts
  const state = creditedState(selected, allIds)
  const labels: Record<string, string> = {
    [CREDITED_ARTIST]: t("filters.artistCredited"),
    [UNCREDITED_ARTIST]: t("filters.artistUncredited"),
  }
  return (
    <FacetFilter
      label={t("filters.artists")}
      groups={[items]}
      counts={counts}
      selected={selected}
      onChange={onChange}
      searchable
      labelOf={(code) => labels[code]}
      isChecked={(code) => artistChecked(selected, code)}
      onToggle={(code) => {
        onChange(toggleArtist(selected, code, allIds))
      }}
      pinned={
        counts?.[CREDITED_ARTIST] || counts?.[UNCREDITED_ARTIST] ? (
          <>
            <ul className="grid grid-cols-2 gap-x-4">
              <li>
                <FacetCheckbox
                  label={labels[CREDITED_ARTIST] ?? ""}
                  count={counts[CREDITED_ARTIST] ?? 0}
                  checked={state === "all"}
                  indeterminate={state === "some"}
                  onChange={() => {
                    onChange(toggleCredited(selected, allIds))
                  }}
                />
              </li>
              {counts[UNCREDITED_ARTIST] ? (
                <li>
                  <FacetCheckbox
                    label={labels[UNCREDITED_ARTIST] ?? ""}
                    count={counts[UNCREDITED_ARTIST]}
                    checked={selected.includes(UNCREDITED_ARTIST)}
                    onChange={() => {
                      onChange(
                        selected.includes(UNCREDITED_ARTIST)
                          ? selected.filter((token) => token !== UNCREDITED_ARTIST)
                          : [...selected, UNCREDITED_ARTIST],
                      )
                    }}
                  />
                </li>
              ) : null}
            </ul>
            <hr className="my-2 border-0 border-t border-dashed border-border-strong" />
          </>
        ) : undefined
      }
    />
  )
}
