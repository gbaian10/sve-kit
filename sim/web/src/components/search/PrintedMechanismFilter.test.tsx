import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
import { PrintedMechanismFilter } from "./PrintedMechanismFilter"

it.each([
  ["zh-TW", "職業機制", "真紅", "全部符合"],
  ["ja", "クラスのメカニズム", "真紅", "すべてに一致"],
  ["en", "Class mechanisms", "Sanguine", "Match all"],
] as const)("supports printed mechanisms in %s", async (language, label, name, all) => {
  const onChange = vi.fn(),
    onMatchChange = vi.fn()
  await renderInRouter(
    <PrintedMechanismFilter
      group="classMechanisms"
      classLabel={(code) => `name:${code}`}
      options={{ coverage: "complete", unknownCards: 0, counts: {} }}
      selected={[]}
      matchMode="any"
      onChange={onChange}
      onMatchChange={onMatchChange}
    />,
    { language },
  )
  await userEvent.click(screen.getByRole("button", { name: new RegExp(`^${label}`, "u") }))
  await userEvent.click(screen.getByRole("button", { name }))
  expect(onChange).toHaveBeenCalledWith(["sanguine"])
  await userEvent.click(screen.getByRole("radio", { name: all }))
  expect(onMatchChange).toHaveBeenCalledWith("all")
})

it("keeps a crossover selection disabled while loading and explains unknown cards", async () => {
  const props = {
    group: "collabMechanisms" as const,
    classLabel: (code: string) => `name:${code}`,
    selected: [],
    matchMode: "any" as const,
    onChange: vi.fn(),
    onMatchChange: vi.fn(),
  }
  const { rerender } = await renderInRouter(
    <PrintedMechanismFilter {...props} options={undefined} />,
    { language: "en" },
  )
  await userEvent.click(screen.getByRole("button", { name: /^Crossover mechanisms/u }))
  expect(screen.getByRole("button", { name: "Serve" })).toBeDisabled()
  rerender(
    <PrintedMechanismFilter
      {...props}
      options={{ coverage: "partial", unknownCards: 3, counts: { serve: 2 } }}
    />,
  )
  await userEvent.click(screen.getByRole("button", { name: /^Crossover mechanisms/u }))
  const group = screen.getByRole("group", { name: "Crossover mechanisms" })
  expect(within(group).getByRole("status")).toHaveTextContent("3 cards are not fully determined")
  expect(within(group).getByRole("button", { name: "Serve2" })).toBeEnabled()
})

it("lays the buttons out in one row per class and title, abbreviations on hover", async () => {
  const props = {
    options: { coverage: "complete" as const, unknownCards: 0, counts: {} },
    selected: [],
    matchMode: "any" as const,
    onChange: vi.fn(),
    onMatchChange: vi.fn(),
    classLabel: (code: string) => `name:${code}`,
  }
  const { rerender } = await renderInRouter(
    <PrintedMechanismFilter {...props} group="classMechanisms" />,
    { language: "zh-TW" },
  )
  await userEvent.click(screen.getByRole("button", { name: /^職業機制/u }))
  const witch = screen.getByRole("group", { name: "name:witch" })
  expect(
    within(witch)
      .getAllByRole("button")
      .map((button) => button.textContent),
  ).toEqual(["魔力連鎖", "蓄積", "土之秘術"])
  expect(screen.getByRole("img", { name: "name:witch" })).toHaveAttribute("title", "name:witch")
  expect(within(witch).getByRole("button", { name: "魔力連鎖" })).toHaveAttribute(
    "title",
    "縮寫 SC（Spellchain）",
  )
  rerender(<PrintedMechanismFilter {...props} group="collabMechanisms" />)
  await userEvent.click(screen.getByRole("button", { name: /^合作作品機制/u }))
  const vanguard = screen.getByRole("group", { name: "卡片戰鬥!! 先導者" })
  expect(within(vanguard).getAllByRole("button")).toHaveLength(3)
  expect(screen.getByTitle("卡片戰鬥!! 先導者")).toHaveTextContent("先導者")
})
