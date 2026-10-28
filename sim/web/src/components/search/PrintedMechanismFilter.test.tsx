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
