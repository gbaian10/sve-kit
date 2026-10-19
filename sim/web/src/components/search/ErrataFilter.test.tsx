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

it("toggles errata independently beside card grouping and round trips through the URL", async () => {
  await renderInRouter(<Harness />)
  const user = userEvent.setup()
  const errata = screen.getByRole("switch", { name: "只顯示勘誤卡" })
  const grouping = screen.getByRole("switch", { name: "同一張卡只顯示一次" })
  expect(errata).toHaveAccessibleDescription("官方發過勘誤公告的卡")
  expect(grouping.compareDocumentPosition(errata) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  await user.click(errata)
  expect(errata).toBeChecked()
  expect(grouping).not.toBeChecked()
  const state = parseQuery(new URLSearchParams(screen.getByLabelText("URL").textContent))
  expect(state.errataOnly).toBe(true)
  await user.click(grouping)
  expect(errata).toBeChecked()
  await user.click(errata)
  expect(screen.getByLabelText("URL")).not.toHaveTextContent("errata")
})
