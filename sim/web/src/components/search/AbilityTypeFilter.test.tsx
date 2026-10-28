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

it.each([
  [
    "zh-TW",
    "能力種類",
    ["Quick", "入場曲", "謝幕曲", "起動", "進階起動"],
    ["融合", "攻擊時", "進化時", "超進化時"],
  ],
  [
    "ja",
    "能力の種類",
    ["クイック", "ファンファーレ", "ラストワード", "起動", "アドバンス起動"],
    ["融合", "攻撃時", "進化時", "超進化時"],
  ],
  [
    "en",
    "Ability types",
    ["Quick", "Fanfare", "Last Words", "Activate", "Advanced Activate"],
    ["Fusion", "Strike", "On Evolve", "On Super-Evolve"],
  ],
] as const)(
  "shows official ability icons and text-only types in %s",
  async (language, label, iconLabels, textLabels) => {
    await renderInRouter(
      <AbilityTypeFilter
        options={{ coverage: "complete", unknownCards: 0 }}
        selected={[]}
        matchMode="any"
        onChange={vi.fn()}
        onMatchChange={vi.fn()}
      />,
      { language },
    )
    await userEvent.click(screen.getByRole("button", { name: new RegExp(`^${label}`, "u") }))
    const group = screen.getByRole("group", { name: label })
    expect(
      within(group)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual([...iconLabels, ...textLabels])
    const iconNames = [
      "icon_quick.png",
      "icon_fanfare.png",
      "icon_lastword.png",
      "icon_stand.png",
      "icon_advancestand.png",
    ]
    iconLabels.forEach((iconLabel, index) => {
      const iconName = iconNames[index]
      if (!iconName) return
      expect(
        within(group).getByRole("button", { name: iconLabel }).querySelector("img"),
      ).toHaveAttribute("src", expect.stringContaining(iconName))
    })
    for (const textLabel of textLabels) {
      expect(within(group).getByRole("button", { name: textLabel }).querySelector("img")).toBeNull()
    }
  },
)

it("uses Quick only for the zh-TW chip and keeps the selected summary unchanged", async () => {
  await renderInRouter(
    <AbilityTypeFilter
      options={{ coverage: "complete", unknownCards: 0 }}
      selected={["quick"]}
      matchMode="any"
      onChange={vi.fn()}
      onMatchChange={vi.fn()}
    />,
    { language: "zh-TW" },
  )
  const trigger = screen.getByRole("button", { name: /^能力種類/u })
  expect(trigger).toHaveTextContent("快速")
  await userEvent.click(trigger)
  expect(screen.getByRole("button", { name: "Quick" })).toBeInTheDocument()
})

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
