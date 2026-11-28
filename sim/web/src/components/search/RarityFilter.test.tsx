import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { describe, expect, it } from "vitest"

import br from "../../assets/rarity/br.svg"
import brEn from "../../assets/rarity/br-en.svg"
import gbf from "../../assets/rarity/gbf.svg"
import pr from "../../assets/rarity/pr.svg"
import prBronze from "../../assets/rarity/pr-bronze.svg"
import type { Region } from "../../domain/search"
import { renderInRouter } from "../../test-utils"
import { RarityFilter } from "./RarityFilter"

function Harness({
  initial = [],
  edition = "jp",
  omitPr = false,
  gbfCount,
}: {
  readonly initial?: string[]
  readonly edition?: Region
  readonly omitPr?: boolean
  readonly gbfCount?: number
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
          ...(gbfCount === undefined ? [] : [{ code: "gbf", label: "GBF" }]),
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
            ...(gbfCount === undefined ? {} : { gbf: gbfCount }),
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
    expect(chips()).toEqual(["BR", "SP", "SSP", "PR", "無分類"])
    const bronze = screen.getByRole("button", { name: "BR" })
    expect(bronze).toHaveAttribute("title", "BR（9）")
    expect(bronze.querySelector("img")).toHaveAttribute("src", br)
    expect(screen.getByRole("button", { name: "PR" }).querySelector("img")).toHaveAttribute(
      "src",
      pr,
    )
    expect(screen.getByRole("button", { name: "無分類" })).toHaveTextContent("無分類")
    expect(screen.getByText("碎閃", { exact: true })).toBeInTheDocument()
    unmount()
    await renderInRouter(<Harness edition="en" />)
    expect(screen.getByRole("button", { name: "BR" }).querySelector("img")).toHaveAttribute(
      "src",
      brEn,
    )
  })

  it("draws GBF after PR and leaves it out of an edition without GBF printings", async () => {
    const { unmount } = await renderInRouter(<Harness gbfCount={3} />)
    expect(chips()).toEqual(["BR", "SP", "SSP", "PR", "GBF", "無分類"])
    expect(screen.getByRole("button", { name: "GBF" }).querySelector("img")).toHaveAttribute(
      "src",
      gbf,
    )
    unmount()
    await renderInRouter(<Harness gbfCount={0} edition="en" />)
    expect(chips()).toEqual(["BR", "SP", "SSP", "PR", "無分類"])
  })

  it("shows premium after SSP even when the vocabulary has no PR option", async () => {
    await renderInRouter(<Harness omitPr />)
    expect(chips()).toEqual(["BR", "SP", "SSP", "無分類"])
  })

  it("renders premium as a switch after the ordinary rarity filters", async () => {
    await renderInRouter(<Harness />)
    const premium = screen.getByRole("switch", { name: "碎閃" })
    const unrated = screen.getByRole("button", { name: "無分類" })
    expect(premium).toHaveAccessibleDescription("只有 BR、SR、GR 與無分類有碎閃；PR 的碎閃尚未收錄")
    expect(unrated.compareDocumentPosition(premium) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(chips()).toEqual(["BR", "SP", "SSP", "PR", "無分類"])
  })

  it.each([
    ["zh-TW", "碎閃", "只有 BR、SR、GR 與無分類有碎閃；PR 的碎閃尚未收錄"],
    [
      "ja",
      "プレミアム",
      "プレミアムはBR・SR・GR・分類なしにのみ存在し、PRのプレミアムは未収録です",
    ],
    [
      "en",
      "Premium",
      "Premium variants exist only for BR, SR, GR, and Unrated; PR Premium is not included yet",
    ],
  ] as const)("shows a premium switch and hint in %s", async (language, label, hint) => {
    await renderInRouter(<Harness />, { language })
    const premium = screen.getByRole("switch", { name: label })
    expect(screen.getByText(label, { exact: true })).toBeInTheDocument()
    expect(premium).toHaveAccessibleDescription(hint)
  })

  it("toggles the premium condition without changing other rarity selections", async () => {
    await renderInRouter(<Harness initial={["sr"]} />)
    const premium = screen.getByRole("switch", { name: "碎閃" })
    expect(premium).not.toBeChecked()
    await userEvent.click(premium)
    expect(premium).toBeChecked()
    expect(screen.getByLabelText("selection")).toHaveTextContent("sr,premium")
    await userEvent.click(premium)
    expect(premium).not.toBeChecked()
    expect(screen.getByLabelText("selection")).toHaveTextContent("sr")
  })

  it("clears the premium condition with the rarity section", async () => {
    await renderInRouter(<Harness />)
    await userEvent.click(screen.getByRole("switch", { name: "碎閃" }))
    expect(screen.getByLabelText("selection")).toHaveTextContent("premium")
    await userEvent.click(screen.getByRole("button", { name: "清除稀有度" }))
    expect(screen.getByLabelText("selection")).toBeEmptyDOMElement()
  })

  it.each([
    [
      "zh-TW",
      "無分類",
      "沒有稀有度的卡，例如 Token 和主戰者。PR 卡包只看卡面右下的稀有度標籤，沒有標籤的版次都歸在「PR（無標籤）」。",
      "無分類（4）",
    ],
    [
      "ja",
      "分類なし",
      "レアリティのないカード（トークン、リーダーなど）。PRパックは右下のレアリティ表記だけで分類し、表記のないカードは「PR（ラベルなし）」に分類されます。",
      "分類なし（4）",
    ],
    [
      "en",
      "Unrated",
      "Cards without a rarity, such as tokens and leaders. PR pack cards are rated only by the badge at the lower right; those without one appear under PR (No badge).",
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
