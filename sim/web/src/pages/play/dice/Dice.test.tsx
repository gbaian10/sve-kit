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
  it("says by how much a value is below the printed one, beside the die", async () => {
    await show(2, 4)
    const die = (await screen.findByRole("img", { name: "體力 2（原本 4）" })).closest(
      ".bp-counter",
    )
    expect(die).toHaveClass("is-down")
    expect(die?.querySelector(".bp-counter-diff")).toHaveTextContent("-2")
  })

  it("says by how much a value is above the printed one", async () => {
    await show(12, 2)
    const die = (await screen.findByRole("img", { name: "體力 12（原本 2）" })).closest(
      ".bp-counter",
    )
    expect(die).toHaveClass("is-up")
    expect(die?.querySelector(".bp-counter-diff")).toHaveTextContent("+10")
  })

  it.each([
    ["the printed value", 4],
    ["no printed value to compare with", undefined],
  ])("marks nothing on a die that shows %s", async (_name, printed) => {
    await show(4, printed)
    const die = (await screen.findByRole("img", { name: "體力 4" })).closest(".bp-counter")
    expect(die).not.toHaveClass("is-up")
    expect(die).not.toHaveClass("is-down")
    expect(die?.querySelector(".bp-counter-diff")).toBeNull()
  })
})
