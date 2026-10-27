import { screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { FilterPanel } from "./FilterPanel"
import { RarityFilter } from "./RarityFilter"

function Harness({ sidebar }: { readonly sidebar: boolean }) {
  const [query, setQuery] = useState(DEFAULT_QUERY)
  return (
    <>
      <FilterPanel
        id="marks"
        open
        sidebar={sidebar}
        query={query}
        onChange={setQuery}
        edition="jp"
        onEditionChange={() => undefined}
        facets={{
          rarity: (
            <RarityFilter
              rarities={[]}
              options={{ styles: [], counts: {} }}
              edition="jp"
              selected={query.rarities}
              onChange={(rarities) => {
                setQuery({ ...query, rarities })
              }}
            />
          ),
          sets: null,
          traits: null,
          artists: null,
          keywords: null,
        }}
      />
      <output aria-label="URL">{formatQuery(query).toString()}</output>
    </>
  )
}

it.each(
  [
    {
      language: "zh-TW" as const,
      stamp: "標誌卡",
      serial: "編號卡",
      premium: "碎閃",
      hint: "只有 PR 卡有",
    },
    {
      language: "ja" as const,
      stamp: "ロゴ入りカード",
      serial: "シリアルナンバー入りカード",
      premium: "プレミアム",
      hint: "PRカードのみ",
    },
    {
      language: "en" as const,
      stamp: "Stamped cards",
      serial: "Numbered cards",
      premium: "Premium",
      hint: "Only PR cards have stamps.",
    },
  ].flatMap((copy) => [true, false].map((sidebar) => ({ ...copy, sidebar }))),
)("filters printing marks in $language (sidebar=$sidebar)", async (copy) => {
  await renderInRouter(<Harness sidebar={copy.sidebar} />, { language: copy.language })
  const user = userEvent.setup()
  const switches = screen.getAllByRole("switch")
  const stamp = screen.getByRole("switch", { name: copy.stamp })
  const serial = screen.getByRole("switch", { name: copy.serial })
  const premium = screen.getByRole("switch", { name: copy.premium })
  expect(switches.indexOf(stamp)).toBe(switches.indexOf(premium) + 1)
  expect(stamp).toHaveAccessibleDescription(copy.hint)
  await user.click(stamp)
  await user.click(serial)
  expect(parseQuery(new URLSearchParams(screen.getByLabelText("URL").textContent))).toMatchObject({
    stampOnly: true,
    serialOnly: true,
    rarities: [],
  })
  await user.click(stamp)
  expect(serial).toBeChecked()
  expect(screen.getByLabelText("URL")).toHaveTextContent("serial=1")
  await user.click(serial)
  expect(screen.getByLabelText("URL").textContent).toBe("")
})
