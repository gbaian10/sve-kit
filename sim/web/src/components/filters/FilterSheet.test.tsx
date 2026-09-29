import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import { DEFAULT_QUERY, type QueryState } from "../../domain/query/model"
import { renderInRouter } from "../../test-utils"
import { FilterSheet } from "./FilterSheet"

const options = {
  types: ["follower", "spell"],
  rarities: ["bronze", "gold"],
  sets: [
    { code: "bp01", id: "set:bp01" },
    { code: "sd01", id: "set:sd01" },
  ],
  keywords: [{ id: "kw:ward", name: () => "守護" }],
}
const classes = [
  { code: "elf", label: "精靈" },
  { code: "neutral", label: "中立" },
]
const label = (kind: "type" | "rarity", code: string) => `${kind}:${code}`

async function open(applied: QueryState = DEFAULT_QUERY) {
  const onApply = vi.fn()
  const onClose = vi.fn()
  const count = vi.fn(
    (state: QueryState) =>
      state.classes.length * 10 +
      (state.altArtOnly ? 1 : 0) +
      Object.keys(state.mechanics).length * 100 +
      (state.text === "" ? 0 : 5),
  )
  await renderInRouter(
    <FilterSheet
      open
      onClose={onClose}
      applied={applied}
      onApply={onApply}
      options={options}
      classes={classes}
      label={label}
      uiLang="zh-Hant"
      count={count}
      coverage={{ annotated: 3, total: 10 }}
    />,
  )
  return { onApply, onClose, count }
}

describe("FilterSheet", () => {
  it("edits a draft, counts it live and applies only on the show button", async () => {
    const { onApply, count } = await open()
    const user = userEvent.setup()
    const dialog = screen.getByRole("dialog")
    expect(within(dialog).getByText("機制已標註 3／10 種卡")).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "精靈" }))
    await user.click(within(dialog).getByRole("switch", { name: "只看異畫" }))
    expect(within(dialog).getByRole("button", { name: "顯示 11 張" })).toBeInTheDocument()
    expect(onApply).not.toHaveBeenCalled()
    expect(count).toHaveBeenLastCalledWith(
      expect.objectContaining({ classes: ["elf"], altArtOnly: true }),
    )
    await user.click(within(dialog).getByRole("button", { name: "顯示 11 張" }))
    expect(onApply).toHaveBeenCalledWith(
      expect.objectContaining({ classes: ["elf"], altArtOnly: true }),
    )
  })

  it("cycles a mechanic through has, lacks and either, and keeps cost as one range", async () => {
    const { onApply } = await open()
    const user = userEvent.setup()
    const dialog = screen.getByRole("dialog")
    const ward = within(dialog).getByRole("button", { name: /守護/u })
    await user.click(ward)
    expect(ward).toHaveTextContent("要有")
    await user.click(ward)
    expect(ward).toHaveTextContent("要沒有")
    await user.click(ward)
    expect(ward).toHaveTextContent("不管")
    await user.click(ward)
    await user.click(within(dialog).getByRole("button", { name: "2" }))
    await user.click(within(dialog).getByRole("button", { name: "7+" }))
    await user.click(within(dialog).getByRole("button", { name: /顯示/u }))
    expect(onApply).toHaveBeenCalledWith(
      expect.objectContaining({ mechanics: { "kw:ward": "has" }, cost: { min: 2, max: 7 } }),
    )
  })

  it("keeps the text and view on reset, so the count is what show applies", async () => {
    const applied: QueryState = { ...DEFAULT_QUERY, text: "bp01", view: "table", classes: ["elf"] }
    const { onApply } = await open(applied)
    const user = userEvent.setup()
    const dialog = screen.getByRole("dialog")
    expect(within(dialog).getByRole("button", { name: "顯示 15 張" })).toBeInTheDocument()
    await user.click(within(dialog).getAllByRole("button", { name: "重設" })[0] ?? dialog)
    expect(within(dialog).getByRole("button", { name: "顯示 5 張" })).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "顯示 5 張" }))
    expect(onApply).toHaveBeenCalledWith({ ...DEFAULT_QUERY, text: "bp01", view: "table" })
  })

  it("resets the draft and keeps the applied text and view when applying", async () => {
    const applied: QueryState = { ...DEFAULT_QUERY, text: "bp01", view: "table", classes: ["elf"] }
    const { onApply, onClose } = await open(applied)
    const user = userEvent.setup()
    const dialog = screen.getByRole("dialog")
    expect(within(dialog).getByRole("button", { name: "精靈" })).toHaveAttribute(
      "aria-pressed",
      "true",
    )
    await user.click(within(dialog).getAllByRole("button", { name: "重設" })[0] ?? dialog)
    expect(within(dialog).getByRole("button", { name: "精靈" })).toHaveAttribute(
      "aria-pressed",
      "false",
    )
    await user.click(within(dialog).getByRole("button", { name: /顯示/u }))
    expect(onApply).toHaveBeenCalledWith({ ...DEFAULT_QUERY, text: "bp01", view: "table" })
    await user.click(within(dialog).getByRole("button", { name: "關閉" }))
    expect(onClose).toHaveBeenCalled()
  })
})
