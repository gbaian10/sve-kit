import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
import { AbilityTypeFilter } from "./AbilityTypeFilter"

it.each([
  ["zh-TW", "能力種類", "入場曲", "全部符合"],
  ["ja", "能力の種類", "ファンファーレ", "すべてに一致"],
  ["en", "Ability types", "Fanfare", "Match all"],
] as const)(
  "shows native ability types and matching controls in %s",
  async (language, label, fanfare, all) => {
    const onChange = vi.fn(),
      onMatchChange = vi.fn()
    await renderInRouter(
      <AbilityTypeFilter
        options={{ coverage: "complete", unknownCards: 0 }}
        selected={[]}
        matchMode="any"
        onChange={onChange}
        onMatchChange={onMatchChange}
      />,
      { language },
    )
    await userEvent.click(screen.getByRole("button", { name: new RegExp(`^${label}`, "u") }))
    await userEvent.click(screen.getByRole("button", { name: fanfare }))
    expect(onChange).toHaveBeenCalledWith(["fanfare"])
    await userEvent.click(screen.getByRole("radio", { name: all }))
    expect(onMatchChange).toHaveBeenCalledWith("all")
  },
)

it("keeps controls disabled while data loads and shows concrete missing coverage", async () => {
  const props = {
    selected: [],
    matchMode: "any" as const,
    onChange: vi.fn(),
    onMatchChange: vi.fn(),
  }
  const { rerender } = await renderInRouter(
    <AbilityTypeFilter {...props} options={{ coverage: "pending" }} />,
    { language: "en" },
  )
  await userEvent.click(screen.getByRole("button", { name: /^Ability types/u }))
  expect(screen.getByRole("button", { name: "Fanfare" })).toBeDisabled()
  rerender(<AbilityTypeFilter {...props} options={{ coverage: "partial", unknownCards: 3 }} />)
  await userEvent.click(screen.getByRole("button", { name: /^Ability types/u }))
  const group = screen.getByRole("group", { name: "Ability types" })
  expect(within(group).getByRole("status")).toHaveTextContent("undetermined for 3 cards")
  expect(screen.getByRole("button", { name: "Fanfare" })).toBeEnabled()
})
