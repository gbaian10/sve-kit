import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { TypeFilter } from "./TypeFilter"

function Harness() {
  const [selected, onChange] = useState(parseQuery(new URLSearchParams("type=spell,token")).types)
  const labels = [
    ["equipment", "裝備"],
    ["sep", "SEP"],
    ["leader", "主戰者"],
    ["amulet", "護符"],
    ["crest", "紋章"],
    ["ep", "EP"],
    ["spell", "法術"],
    ["follower", "從者"],
  ] as const
  return (
    <>
      <TypeFilter
        textLang="zh-Hant"
        selected={selected}
        onChange={onChange}
        options={{
          types: labels.map(([code, label]) => ({
            id: code,
            code,
            name: { original: { lang: "ja", text: code }, translations: { "zh-Hant": label } },
          })),
          typeCounts: { follower: 3, spell: 2, amulet: 1, crest: 1, leader: 2, ep: 1, sep: 1 },
        }}
      />
      <output aria-label="url">
        {formatQuery({ ...DEFAULT_QUERY, types: selected }).toString()}
      </output>
    </>
  )
}

it("offers only the printed types in order and leaves deck locations to their own row", async () => {
  await renderInRouter(<Harness />)
  const row = screen.getByRole("group", { name: "種類" })
  expect(
    within(row)
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label")),
  ).toEqual(["清除種類", "從者", "法術", "護符"])
  // `token` in an older URL is a deck location, so it never reaches this row's selection.
  expect(within(row).getByRole("button", { name: "法術" })).toHaveAttribute("aria-pressed", "true")
  expect(screen.getByLabelText("url")).toHaveTextContent(/^type=spell$/u)
  await userEvent.click(within(row).getByRole("button", { name: "從者" }))
  expect(parseQuery(new URLSearchParams(screen.getByLabelText("url").textContent)).types).toEqual([
    "follower",
    "spell",
  ])
  await userEvent.click(within(row).getByRole("button", { name: "清除種類" }))
  expect(screen.getByLabelText("url")).toBeEmptyDOMElement()
})
