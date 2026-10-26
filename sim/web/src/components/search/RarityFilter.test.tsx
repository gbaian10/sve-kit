import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { describe, expect, it } from "vitest"

import br from "../../assets/rarity/br.svg"
import brEn from "../../assets/rarity/br-en.svg"
import pr from "../../assets/rarity/pr.svg"
import prBronze from "../../assets/rarity/pr-bronze.svg"
import type { Region } from "../../domain/search"
import { renderInRouter } from "../../test-utils"
import { RarityFilter } from "./RarityFilter"

function Harness({
  initial = [],
  edition = "jp",
  omitPr = false,
}: {
  readonly initial?: string[]
  readonly edition?: Region
  readonly omitPr?: boolean
}) {
  const [selected, onChange] = useState(initial)
  return (
    <>
      <RarityFilter
        rarities={[
          { code: "sp", label: "SP" },
          { code: "ssp", label: "SSP" },
          { code: "pr", label: "PR" },
          { code: "br", label: "BR" },
        ].filter((rarity) => !omitPr || rarity.code !== "pr")}
        options={{
          styles: ["br", "none", "unknown"],
          counts: {
            sp: 1,
            ssp: 2,
            premium: 35,
            pr: 6,
            "pr:br": 3,
            "pr:none": 2,
            "pr:unknown": 1,
            unrated: 4,
            br: 9,
          },
        }}
        edition={edition}
        selected={selected}
        onChange={onChange}
      />
      <output aria-label="selection">{selected.join(",")}</output>
    </>
  )
}

const chips = () =>
  within(screen.getByRole("group", { name: "稀有度" }))
    .getAllByRole("button", { pressed: false })
    .map((button) => button.getAttribute("aria-label"))

describe("rarity chips", () => {
  it("orders the chips low to high and draws the edition's badge on each", async () => {
    const { unmount } = await renderInRouter(<Harness />)
    expect(chips()).toEqual(["BR", "SP", "SSP", "PR", "無分類", "碎閃"])
    const bronze = screen.getByRole("button", { name: "BR" })
    expect(bronze).toHaveAttribute("title", "BR（9）")
    expect(bronze.querySelector("img")).toHaveAttribute("src", br)
    expect(screen.getByRole("button", { name: "PR" }).querySelector("img")).toHaveAttribute(
      "src",
      pr,
    )
    expect(screen.getByRole("button", { name: "無分類" })).toHaveTextContent("無分類")
    expect(screen.getByRole("button", { name: "碎閃" })).toHaveTextContent("碎閃")
    expect(screen.getByRole("button", { name: "碎閃" }).querySelector("img")).toBeNull()
    unmount()
    await renderInRouter(<Harness edition="en" />)
    expect(screen.getByRole("button", { name: "BR" }).querySelector("img")).toHaveAttribute(
      "src",
      brEn,
    )
  })

  it("shows premium after SSP even when the vocabulary has no PR option", async () => {
    await renderInRouter(<Harness omitPr />)
    expect(chips()).toEqual(["BR", "SP", "SSP", "無分類", "碎閃"])
  })

  it("puts premium on its own row after the ordinary rarity filters", async () => {
    await renderInRouter(<Harness />)
    const group = screen.getByRole("group", { name: "稀有度" })
    const premium = screen.getByRole("button", { name: "碎閃" })
    const isolatedRow = group.querySelector('[data-filter-chip-row="isolated"]')
    expect(isolatedRow).toContainElement(premium)
    expect(isolatedRow).toHaveClass("basis-full")
    expect(isolatedRow?.querySelectorAll("button")).toHaveLength(1)
    expect(chips()).toEqual(["BR", "SP", "SSP", "PR", "無分類", "碎閃"])
  })

  it.each([
    ["zh-TW", "碎閃"],
    ["ja", "プレミアム"],
    ["en", "Premium"],
  ] as const)("shows a text-only premium chip in %s", async (language, label) => {
    await renderInRouter(<Harness />, { language })
    const premium = screen.getByRole("button", { name: label })
    expect(premium).toHaveTextContent(label)
    expect(premium.querySelector("img")).toBeNull()
  })

  it("selects the premium token and clears it with the rarity section", async () => {
    await renderInRouter(<Harness />)
    await userEvent.click(screen.getByRole("button", { name: "碎閃" }))
    expect(screen.getByLabelText("selection")).toHaveTextContent("premium")
    await userEvent.click(screen.getByRole("button", { name: "清除稀有度" }))
    expect(screen.getByLabelText("selection")).toBeEmptyDOMElement()
  })

  it.each([
    [
      "zh-TW",
      "無分類",
      "官方沒有登記稀有度的卡，例如 Token、主戰者，以及 PR 系列的 EP、SEP",
      "無分類（4）",
    ],
    [
      "ja",
      "分類なし",
      "公式のレアリティ表記がないカード（トークン、リーダー、PRのEP・SEPなど）",
      "分類なし（4）",
    ],
    [
      "en",
      "Unrated",
      "Cards with no official rarity, such as Tokens, leaders, and PR-series EP and SEP cards",
      "Unrated (4)",
    ],
  ] as const)(
    "explains the unrated chip in %s to hover and assistive technology only",
    async (language, label, hint, chipTitle) => {
      await renderInRouter(<Harness />, { language })
      const unrated = screen.getByRole("button", { name: label })
      expect(unrated).toHaveAccessibleDescription(hint)
      expect(unrated).toHaveAttribute("title", `${chipTitle}\n${hint}`)
      expect(screen.getByRole("button", { name: "SP" })).not.toHaveAttribute("aria-describedby")
    },
  )

  it("selects or clears every PR appearance from the chip and shows a partial selection", async () => {
    await renderInRouter(<Harness />)
    const parent = screen.getByRole("button", { name: "PR" })
    const arrow = screen.getByRole("button", { name: "PR的細項" })
    expect(arrow).toHaveAttribute("aria-expanded", "false")
    const popover = document.getElementById(arrow.getAttribute("aria-controls") ?? "")
    expect(popover).not.toBeVisible()
    await userEvent.click(parent)
    expect(screen.getByLabelText("selection")).toHaveTextContent(/^pr$/u)
    expect(parent).toHaveAttribute("aria-pressed", "true")
    await userEvent.click(arrow)
    expect(arrow).toHaveAttribute("aria-expanded", "true")
    expect(popover).toBeVisible()
    const group = screen.getByRole("group", { name: "PR的細項" })
    expect(within(group).queryByRole("checkbox", { name: "PR（SR）" })).not.toBeInTheDocument()
    const bronze = within(group).getByRole("checkbox", { name: "PR（BR）" })
    expect(bronze).toBeChecked()
    expect(bronze).toHaveAccessibleDescription("3")
    expect(bronze.closest("label")?.querySelector("img")).toHaveAttribute("src", prBronze)
    await userEvent.click(within(group).getByRole("checkbox", { name: "PR（無標籤）" }))
    expect(parent).toHaveAttribute("aria-pressed", "mixed")
    expect(screen.getByLabelText("selection")).toHaveTextContent("pr:br,pr:unknown")
    await userEvent.click(screen.getByRole("button", { name: "SP" }))
    await userEvent.click(screen.getByRole("button", { name: "無分類" }))
    expect(screen.getByLabelText("selection")).toHaveTextContent("pr:br,pr:unknown,sp,unrated")
    await userEvent.click(parent)
    expect(screen.getByLabelText("selection")).toHaveTextContent("pr,sp,unrated")
    await userEvent.click(parent)
    expect(screen.getByLabelText("selection")).toHaveTextContent("sp,unrated")
    expect(parent).toHaveAttribute("aria-pressed", "false")
  })

  it("closes the popover with Escape or a click outside and returns focus to the arrow", async () => {
    await renderInRouter(<Harness initial={["pr:br"]} />)
    const arrow = screen.getByRole("button", { name: "PR的細項" })
    expect(screen.getByRole("button", { name: "PR" })).toHaveAttribute("aria-pressed", "mixed")
    await userEvent.click(arrow)
    expect(screen.getByRole("checkbox", { name: "PR（BR）" })).toHaveFocus()
    await userEvent.keyboard("{Escape}")
    expect(arrow).toHaveAttribute("aria-expanded", "false")
    expect(arrow).toHaveFocus()
    await userEvent.click(arrow)
    expect(arrow).toHaveAttribute("aria-expanded", "true")
    await userEvent.click(screen.getByLabelText("selection"))
    expect(arrow).toHaveAttribute("aria-expanded", "false")
  })
})
