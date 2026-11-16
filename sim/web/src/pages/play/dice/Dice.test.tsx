import { screen } from "@testing-library/react"
import { beforeEach, describe, expect, it } from "vitest"

import { prefsStore } from "../../../settings"
import { renderRoutes } from "../../../test-utils"
import { CounterBadge } from "./Dice"

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

const show = (value: number, printed: number | null | undefined) =>
  renderRoutes([
    {
      path: "*",
      element: (
        <CounterBadge
          kind="hp"
          value={value}
          className="bp-fstat bp-def"
          label="體力"
          printed={printed}
        />
      ),
    },
  ])

describe("CounterBadge", () => {
  it("shows a moved value as the number alone, and says the printed one on hover", async () => {
    await show(2, 4)
    const die = (await screen.findByRole("img", { name: "體力 2（原本 4）" })).closest(
      ".bp-counter",
    )
    // No tag beside the die and no ring around it: nothing but the number marks the change.
    expect(die?.children).toHaveLength(1)
    expect(die?.textContent).toBe("")
    expect(die?.className).toBe("bp-counter bp-fstat bp-def")
    expect(die).toHaveAttribute("title", "體力 2（原本 4）")
  })

  it.each([
    ["the printed value", 4],
    ["no printed value to compare with", undefined],
  ])("names only the value on a die that shows %s", async (_name, printed) => {
    await show(4, printed)
    const die = (await screen.findByRole("img", { name: "體力 4" })).closest(".bp-counter")
    expect(die?.className).toBe("bp-counter bp-fstat bp-def")
  })
})
