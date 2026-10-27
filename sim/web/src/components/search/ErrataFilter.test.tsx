import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { FilterPanel } from "./FilterPanel"

function Harness({ covered = false }: { readonly covered?: boolean }) {
  const [query, setQuery] = useState(DEFAULT_QUERY)
  return (
    <>
      <FilterPanel
        id="test-filters"
        open
        query={query}
        onChange={setQuery}
        edition="jp"
        onEditionChange={() => undefined}
        facets={null}
        restrictions={
          covered ? { state: "partial", onDate: "2026-09-29", synthetic: true } : undefined
        }
      />
      <output aria-label="URL">{formatQuery(query).toString()}</output>
    </>
  )
}

it("toggles errata independently before the display controls and round trips through the URL", async () => {
  await renderInRouter(<Harness />)
  const user = userEvent.setup()
  const errata = screen.getByRole("switch", { name: "只顯示勘誤卡" })
  const grouping = screen.getByRole("combobox", { name: "顯示方式" })
  const checkbox = screen.getByRole("checkbox")
  expect(grouping).toHaveClass("focus-visible:outline-offset-[-2px]")
  expect(checkbox).toHaveClass("focus-visible:outline-offset-[-2px]")
  const displayHeading = screen.getByText("顯示方式", { selector: "p" })
  expect(screen.getByRole("switch", { name: "只顯示禁限卡" })).toBeDisabled()
  expect(errata).toHaveAccessibleDescription("官方發過勘誤公告的卡")
  expect(errata.compareDocumentPosition(grouping) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  expect(
    displayHeading.compareDocumentPosition(grouping) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  await user.click(errata)
  expect(errata).toBeChecked()
  expect(grouping).toHaveValue("printing")
  const state = parseQuery(new URLSearchParams(screen.getByLabelText("URL").textContent))
  expect(state.errataOnly).toBe(true)
  await user.selectOptions(grouping, "card")
  expect(errata).toBeChecked()
  await user.click(errata)
  expect(screen.getByLabelText("URL")).not.toHaveTextContent("errata")
})

it.each([
  {
    language: "zh-TW" as const,
    group: "再錄卡",
    hide: "不含再錄",
    only: "只看再錄",
    display: "顯示方式",
    note: "不適用",
  },
  {
    language: "ja" as const,
    group: "再録カード",
    hide: "再録を除く",
    only: "再録のみ",
    display: "表示方法",
    note: "無効",
  },
  {
    language: "en" as const,
    group: "Reprints",
    hide: "Hide reprints",
    only: "Reprints only",
    display: "Display",
    note: "Not applied",
  },
])("switches the reprint filter in $language and ignores it per card", async (copy) => {
  await renderInRouter(<Harness />, { language: copy.language })
  const user = userEvent.setup()
  const group = screen.getByRole("radiogroup", { name: copy.group })
  expect(within(group).getAllByRole("radio")).toHaveLength(3)
  expect(within(group).getAllByRole("radio")[0]).toBeChecked()
  expect(screen.getByLabelText("URL")).toHaveTextContent("")
  await user.click(screen.getByRole("radio", { name: copy.hide }))
  expect(screen.getByLabelText("URL")).toHaveTextContent("reprint=hide")
  await user.click(screen.getByRole("radio", { name: copy.only }))
  const url = screen.getByLabelText("URL").textContent
  expect(url).toBe("reprint=only")
  expect(parseQuery(new URLSearchParams(url))).toMatchObject({ reprints: "only" })
  await user.selectOptions(screen.getByRole("combobox", { name: copy.display }), "card")
  expect(screen.getByRole("radio", { name: copy.only })).toBeDisabled()
  expect(screen.getByRole("radio", { name: copy.only })).toBeChecked()
  expect(group).toHaveAccessibleDescription(new RegExp(copy.note, "u"))
})

it.each([
  { language: "zh-TW" as const, label: "只顯示禁限卡", unknown: "來源尚有缺口" },
  { language: "ja" as const, label: "禁止・制限カードのみ", unknown: "情報に欠落" },
  {
    language: "en" as const,
    label: "Only show restricted cards",
    unknown: "source coverage is incomplete",
  },
])(
  "round trips restricted filtering and exposes partial coverage in $language",
  async ({ language, label, unknown }) => {
    await renderInRouter(<Harness covered />, { language })
    const user = userEvent.setup()
    const control = screen.getByRole("switch", { name: label })
    expect(control).toHaveAccessibleDescription(new RegExp(unknown, "u"))
    await user.click(control)
    expect(
      parseQuery(new URLSearchParams(screen.getByLabelText("URL").textContent)).restrictionsOnly,
    ).toBe(true)
    expect(screen.getByLabelText("URL")).toHaveTextContent("restricted=1")
    await user.click(control)
    expect(screen.getByLabelText("URL")).not.toHaveTextContent("restricted")
  },
)
