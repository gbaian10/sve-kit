import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it } from "vitest"

import type { SearchOptions } from "../../data"
import type { NameSource } from "../../domain/nameDisplay"
import { formatQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { SetFilter } from "./SetFilter"

const name = (text: string): NameSource => ({
  original: { lang: "ja", text },
  translations: { "zh-Hant": `譯名${text}`, en: `English ${text}` },
})

const sets = [
  { code: "promo-a", publicCode: "PX01", kind: "promo", name: name("宣傳甲") },
  { code: "collab-a", publicCode: "CX01", kind: "collaboration", name: name("合作甲") },
  { code: "booster-a", publicCode: "BX01", kind: "booster", name: name("補充甲") },
  { code: "other-a", publicCode: "SX01", kind: "special", name: name("其他甲") },
  { code: "promo-b", publicCode: "PX02", kind: "promo", name: name("宣傳乙") },
  { code: "booster-b", publicCode: "BX02", kind: "booster", name: name("補充乙") },
  { code: "collab-b", publicCode: "CX02", kind: "collaboration", name: name("合作乙") },
  { code: "other-b", publicCode: "SX02", kind: "other", name: name("其他乙") },
]

const asSearchOption = (set: (typeof sets)[number] | undefined) => {
  if (!set) throw new Error("Missing synthetic set")
  return { ...set, id: set.code }
}

const quickSets: SearchOptions["quickSets"] = {
  recent: asSearchOption(sets[2]),
  promo: asSearchOption(sets[0]),
}

function Harness({
  items = sets,
  quick = quickSets,
}: {
  readonly items?: typeof sets
  readonly quick?: SearchOptions["quickSets"]
}) {
  const [selected, setSelected] = useState<string[]>([])
  return (
    <>
      <SetFilter
        sets={items}
        quickSets={quick}
        counts={Object.fromEntries(items.map((set) => [set.code, 1]))}
        textLang="zh-Hant"
        selected={selected}
        onChange={setSelected}
      />
      <output aria-label="url">
        {formatQuery({ ...DEFAULT_QUERY, sets: selected }).toString()}
      </output>
    </>
  )
}

async function openFilter(items = sets, quick = quickSets) {
  await renderInRouter(<Harness items={items} quick={quick} />)
  const button = screen.getByRole("button", { name: /^卡包/u })
  await userEvent.click(button)
  return document.getElementById(button.getAttribute("aria-controls") ?? "")
}

it("groups synthetic sets by family kind and puts promo first within the remaining group", async () => {
  const dropdown = await openFilter()
  const lists = within(dropdown as HTMLElement).getAllByRole("list")
  expect(
    lists.map((list) =>
      within(list)
        .getAllByRole("checkbox")
        .map((checkbox) => checkbox.getAttribute("aria-label")),
    ),
  ).toEqual([
    ["BX01 譯名補充甲", "BX02 譯名補充乙"],
    ["CX01 譯名合作甲", "CX02 譯名合作乙"],
    ["PX01 譯名宣傳甲", "PX02 譯名宣傳乙", "SX01 譯名其他甲", "SX02 譯名其他乙"],
  ])
  expect(within(dropdown as HTMLElement).getAllByRole("separator")).toHaveLength(2)
})

it("keeps quick buttons and dropdown selections in sync with the URL", async () => {
  await renderInRouter(<Harness />)
  const latest = screen.getByRole("button", { name: "最新卡包：譯名補充甲" })
  const promo = screen.getByRole("button", { name: "PX01 譯名宣傳甲" })
  await userEvent.click(latest)
  expect(latest).toHaveAttribute("aria-pressed", "true")
  expect(screen.getByLabelText("url")).toHaveTextContent(/^set=booster-a$/u)

  const dropdownButton = screen.getByRole("button", { name: /^卡包/u })
  await userEvent.click(dropdownButton)
  const dropdown = document.getElementById(dropdownButton.getAttribute("aria-controls") ?? "")
  const latestCheckbox = within(dropdown as HTMLElement).getByRole("checkbox", {
    name: "BX01 譯名補充甲",
  })
  expect(latestCheckbox).toBeChecked()
  await userEvent.click(
    within(dropdown as HTMLElement).getByRole("checkbox", { name: "PX01 譯名宣傳甲" }),
  )
  expect(promo).toHaveAttribute("aria-pressed", "true")
  expect(screen.getByLabelText("url")).toHaveTextContent(/^set=booster-a%2Cpromo-a$/u)
  await userEvent.click(latest)
  expect(latest).toHaveAttribute("aria-pressed", "false")
  expect(promo).toHaveAttribute("aria-pressed", "true")
})

it("hides quick buttons when no recent set or unique promo is available", async () => {
  const noQuickSets: SearchOptions["quickSets"] = {}
  await renderInRouter(<Harness quick={noQuickSets} />)
  expect(screen.queryByRole("button", { name: /最新/u })).not.toBeInTheDocument()
  expect(screen.queryByRole("button", { name: "PX01 譯名宣傳甲" })).not.toBeInTheDocument()
})

it("omits empty groups without leaving extra separators", async () => {
  const dropdown = await openFilter(sets.filter((set) => set.kind !== "collaboration"))
  expect(within(dropdown as HTMLElement).getAllByRole("list")).toHaveLength(2)
  expect(within(dropdown as HTMLElement).getAllByRole("separator")).toHaveLength(1)
})
