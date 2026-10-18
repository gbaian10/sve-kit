import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { TraitFilter } from "./TraitFilter"

const TRAITS = [
  ["pixie", "妖精", "Pixie"],
  ["departed", "死者", "Departed"],
  ["machina", "機械", "Machina"],
  ["new_year", "", "New Year"],
] as const

function Harness({ textLang }: { textLang: "zh-Hant" | "en" }) {
  const [selected, onChange] = useState(parseQuery(new URLSearchParams("trait=machina")).traits)
  return (
    <>
      <TraitFilter
        textLang={textLang}
        selected={selected}
        onChange={onChange}
        options={{
          traits: TRAITS.map(([code, zh, en]) => ({
            id: `trait:${code}`,
            code,
            name: {
              original: { lang: "ja", text: `ja-${code}` },
              translations: { ...(zh === "" ? {} : { "zh-Hant": zh }), en },
            },
          })),
          counts: { pixie: 12, departed: 7, machina: 3, new_year: 1 },
        }}
      />
      <output aria-label="url">
        {formatQuery({ ...DEFAULT_QUERY, traits: selected }).toString()}
      </output>
    </>
  )
}

const url = () => decodeURIComponent(screen.getByLabelText("url").textContent)

it("lists traits by their label, narrows them from the search box and keeps selections", async () => {
  await renderInRouter(<Harness textLang="zh-Hant" />)
  const button = screen.getByRole("button", { name: /^特性/u })
  expect(button).toHaveTextContent("已選 1 項")
  await userEvent.click(button)
  const list = screen.getByRole("group", { name: "特性" })
  // Ordered by the label in the UI's text language; a trait without a translation shows its
  // original label.
  expect(
    within(list)
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.getAttribute("aria-label")),
  ).toEqual(["死者", "妖精", "機械", "ja-new_year"])
  expect(within(list).getByRole("checkbox", { name: "機械" })).toBeChecked()
  expect(within(list).getByRole("checkbox", { name: "妖精" })).toHaveAccessibleDescription("12")
  await userEvent.type(screen.getByRole("searchbox", { name: "搜尋特性" }), "妖")
  expect(
    within(list)
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.getAttribute("aria-label")),
  ).toEqual(["妖精", "機械"])
  await userEvent.click(within(list).getByRole("checkbox", { name: "妖精" }))
  expect(url()).toBe("trait=machina,pixie")
  await userEvent.clear(screen.getByRole("searchbox", { name: "搜尋特性" }))
  await userEvent.type(screen.getByRole("searchbox", { name: "搜尋特性" }), "zzz")
  expect(screen.getByText("沒有符合的項目")).toBeInTheDocument()
  expect(
    within(list)
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.getAttribute("aria-label")),
  ).toEqual(["妖精", "機械"])
  await userEvent.click(within(list).getByRole("checkbox", { name: "機械" }))
  expect(parseQuery(new URLSearchParams(url())).traits).toEqual(["pixie"])
})

it("matches the search box against width-folded English labels", async () => {
  await renderInRouter(<Harness textLang="en" />)
  await userEvent.click(screen.getByRole("button", { name: /^特性/u }))
  await userEvent.type(screen.getByRole("searchbox", { name: "搜尋特性" }), "ＮＥＷ")
  const list = screen.getByRole("group", { name: "特性" })
  expect(
    within(list)
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.getAttribute("aria-label")),
  ).toEqual(["Machina", "New Year"])
})
