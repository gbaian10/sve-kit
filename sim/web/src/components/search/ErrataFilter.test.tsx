import { screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { FilterPanel } from "./FilterPanel"

function Harness() {
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
  expect(screen.getAllByRole("switch")).toHaveLength(1)
  expect(errata).toHaveAccessibleDescription("官方發過勘誤公告的卡")
  expect(errata.compareDocumentPosition(grouping) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
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
  { language: "zh-TW" as const, label: "不顯示再錄卡", display: "顯示方式", note: "不適用" },
  { language: "ja" as const, label: "再録カードを隠す", display: "表示方法", note: "無効" },
  { language: "en" as const, label: "Hide reprints", display: "Display", note: "Not applied" },
])("toggles hiding reprints in $language and ignores it per card", async (copy) => {
  await renderInRouter(<Harness />, { language: copy.language })
  const user = userEvent.setup()
  const hide = screen.getByRole("checkbox", { name: copy.label })
  expect(hide).not.toBeChecked()
  expect(screen.getByLabelText("URL")).toHaveTextContent("")
  await user.click(hide)
  expect(hide).toBeChecked()
  const url = screen.getByLabelText("URL").textContent
  expect(url).toBe("noreprint=1")
  expect(parseQuery(new URLSearchParams(url))).toMatchObject({ hideReprints: true })
  await user.selectOptions(screen.getByRole("combobox", { name: copy.display }), "card")
  expect(hide).toBeDisabled()
  expect(hide).toBeChecked()
  expect(hide).toHaveAccessibleDescription(new RegExp(copy.note, "u"))
})
