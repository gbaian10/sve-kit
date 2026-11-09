import { fireEvent, screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { ConditionChips } from "./ConditionChips"
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
          traits: null,
          artists: null,
          keywords: null,
        }}
      />
      <ConditionChips
        query={query}
        edition="jp"
        classLabel={(code) => code}
        rarityLabel={(code) => code}
        typeLabel={(code) => code}
        keywordLabel={(code) => code}
        traitLabel={(code) => code}
        artistLabel={(code) => code}
        setLabel={(code) => code}
        groups={{}}
        onChange={setQuery}
        onClear={() => setQuery(DEFAULT_QUERY)}
      />
      <output aria-label="URL">{formatQuery(query).toString()}</output>
    </>
  )
}

it.each(["標誌卡", "編號卡", "官方未收錄"])(
  "keeps the dropdown open when the first %s condition appears and disappears",
  async (label) => {
    await renderInRouter(<Harness sidebar />)
    const user = userEvent.setup()
    expect(screen.queryByRole("group", { name: "目前條件" })).not.toBeInTheDocument()
    const trigger = screen.getByRole("button", { name: /^特殊版次:/u })
    await user.click(trigger)
    const choice = screen.getByRole("checkbox", { name: label })
    await user.click(choice)
    expect(choice).toBeChecked()
    expect(screen.getByRole("group", { name: "目前條件" })).toBeVisible()
    // The browser adjusts scroll when the condition row and shorter results change page height.
    fireEvent.scroll(window)
    expect(trigger).toHaveAttribute("aria-expanded", "true")
    await user.click(choice)
    expect(choice).not.toBeChecked()
    expect(screen.queryByRole("group", { name: "目前條件" })).not.toBeInTheDocument()
    fireEvent.scroll(window)
    expect(trigger).toHaveAttribute("aria-expanded", "true")
  },
)

it.each(
  [
    {
      language: "zh-TW" as const,
      dropdown: "特殊版次",
      stamp: "標誌卡",
      serial: "編號卡",
      unlisted: "官方未收錄",
      premium: "碎閃",
      stampHint: "只有 PR 卡有",
      serialHint: "卡面印有序號牌",
      unlistedHint: "官方卡表沒有的 EP／SEP 等",
      intersection: "勾選多項時只顯示同時符合的卡",
      more: "其他",
    },
    {
      language: "ja" as const,
      dropdown: "特殊版",
      stamp: "ロゴ入りカード",
      serial: "シリアルナンバー入りカード",
      unlisted: "公式未収録",
      premium: "プレミアム",
      stampHint: "PRカードのみ",
      serialHint: "カードにシリアルナンバーが印字されている",
      unlistedHint: "公式カードリストにないEP／SEPなど",
      intersection: "複数選ぶと、すべてに当てはまるカードだけを表示",
      more: "その他",
    },
    {
      language: "en" as const,
      dropdown: "Special printings",
      stamp: "Stamped cards",
      serial: "Numbered cards",
      unlisted: "Not in official database",
      premium: "Premium",
      stampHint: "Only PR cards have stamps.",
      serialHint: "Printed with a serial-number plate",
      unlistedHint: "EP/SEP and similar cards missing from the official card list",
      intersection: "Selecting several shows only cards that match all of them",
      more: "More",
    },
  ].flatMap((copy) => [true, false].map((sidebar) => ({ ...copy, sidebar }))),
)("filters special printings from one dropdown in $language (sidebar=$sidebar)", async (copy) => {
  await renderInRouter(<Harness sidebar={copy.sidebar} />, { language: copy.language })
  const user = userEvent.setup()
  // The old switches are gone from the printing group; premium stays a switch beside the rarity.
  expect(screen.queryByRole("switch", { name: copy.stamp })).toBeNull()
  expect(screen.queryByRole("switch", { name: copy.serial })).toBeNull()
  expect(screen.getByRole("switch", { name: copy.premium })).toBeInTheDocument()
  // The dropdown sits in the "more" section.
  const more = screen.getByRole("heading", { name: copy.more }).closest("section")
  if (!more) throw new Error("missing other filters section")
  const trigger = within(more).getByRole("button", {
    name: new RegExp(`^${copy.dropdown}:`, "u"),
  })
  expect(trigger).toHaveTextContent(/全部|すべて|All/u)
  await user.click(trigger)
  const popup = document.getElementById(trigger.getAttribute("aria-controls") ?? "")
  if (!popup) throw new Error("missing dropdown popup")
  const panel = within(popup)
  const stamp = panel.getByRole("checkbox", { name: copy.stamp })
  const serial = panel.getByRole("checkbox", { name: copy.serial })
  const unlisted = panel.getByRole("checkbox", { name: copy.unlisted })
  expect(stamp).toHaveAccessibleDescription(copy.stampHint)
  expect(serial).toHaveAccessibleDescription(copy.serialHint)
  expect(unlisted).toHaveAccessibleDescription(copy.unlistedHint)
  expect(panel.getByText(copy.intersection)).toBeInTheDocument()
  await user.click(stamp)
  await user.click(serial)
  await user.click(unlisted)
  expect(parseQuery(new URLSearchParams(screen.getByLabelText("URL").textContent))).toMatchObject({
    stampOnly: true,
    serialOnly: true,
    unlistedOnly: true,
    rarities: [],
  })
  expect(screen.getByLabelText("URL")).toHaveTextContent("unlisted=1")
  expect(trigger).toHaveTextContent(`${copy.stamp}、${copy.serial}、${copy.unlisted}`)
  await user.click(stamp)
  expect(serial).toBeChecked()
  expect(unlisted).toBeChecked()
  expect(screen.getByLabelText("URL")).toHaveTextContent("serial=1")
  await user.click(serial)
  await user.click(unlisted)
  expect(screen.getByLabelText("URL").textContent).toBe("")
})
