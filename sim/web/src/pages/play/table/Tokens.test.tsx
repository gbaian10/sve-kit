import { fireEvent, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it } from "vitest"

import type { TableCard } from "../../../battle/view/table"
import { prefsStore } from "../../../settings"
import { renderRoutes } from "../../../test-utils"
import { CounterTokens } from "./Tokens"

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

const holding = (counters: readonly { name: string; amount: number }[]): TableCard => ({
  handle: "a1",
  card: { printing_id: "p:test", face: 0 },
  face: { printing_id: "p:test", face: 0 },
  faceDown: false,
  acted: false,
  evolution: "base",
  stats: null,
  keywords: [],
  counters,
  cost: null,
  attack: null,
})

const show = (counters: readonly { name: string; amount: number }[]) =>
  renderRoutes([{ path: "*", element: <CounterTokens card={holding(counters)} /> }])

const art = (chip: HTMLElement): Element => {
  const picture = chip.querySelector(".bp-token-art")
  if (picture === null) throw new Error("the chip has no picture")
  return picture
}

describe("CounterTokens", () => {
  it("shows one chip per kind, side by side: the back for the first, the front for a second", async () => {
    await show([
      { name: "スタックカウンター", amount: 2 },
      { name: "融合カウンター", amount: 12 },
    ])
    const stack = await screen.findByRole("img", { name: "指示物：蓄積 ×2" })
    const fusion = screen.getByRole("img", { name: "指示物：融合 ×12" })
    // The pictures are published beside the snapshot root; nothing of them is in the app.
    expect(art(stack)).toHaveAttribute("src", expect.stringMatching(/\/ui\/counter_chip_b\.webp$/))
    expect(art(fusion)).toHaveAttribute("src", expect.stringMatching(/\/ui\/counter_chip_a\.webp$/))
    expect(stack).toHaveTextContent(/^2$/)
    expect(fusion).toHaveTextContent(/^12$/)
    expect(stack.parentElement).toHaveClass("is-several")
  })

  it("counts a kind it has no name for without printing its rules name", async () => {
    await show([{ name: "ナゾカウンター", amount: 4 }])
    const chip = await screen.findByRole("img", { name: "指示物：指示物 ×4" })
    expect(chip).toHaveAttribute("title", "指示物 ×4")
    expect(chip).toHaveTextContent(/^4$/)
    expect(chip.parentElement).not.toHaveClass("is-several")
  })

  it("draws a chip of its own everywhere once the published one fails to load", async () => {
    await renderRoutes([
      {
        path: "*",
        element: (
          <>
            <CounterTokens card={holding([{ name: "返戻カウンター", amount: 1 }])} />
            <CounterTokens card={holding([{ name: "返戻カウンター", amount: 8 }])} />
          </>
        ),
      },
    ])
    const [first, second] = await screen.findAllByRole("img", { name: /^指示物：返還/ })
    if (first === undefined || second === undefined) throw new Error("no chips")
    expect([art(first).tagName, art(second).tagName]).toEqual(["IMG", "IMG"])
    fireEvent.error(art(first))
    expect([art(first).tagName, art(second).tagName]).toEqual(["svg", "svg"])
    // The number stays on the drawn chip.
    expect(second).toHaveTextContent(/^8$/)
  })

  it("marks three kinds or more, whose chips are drawn smallest", async () => {
    await show([
      { name: "スタックカウンター", amount: 1 },
      { name: "融合カウンター", amount: 1 },
      { name: "返戻カウンター", amount: 1 },
    ])
    const [first] = await screen.findAllByRole("img", { name: /^指示物：/ })
    expect(first?.parentElement).toHaveClass("is-many")
    expect(first?.parentElement).not.toHaveClass("is-several")
  })

  it("draws nothing for a card without counters", async () => {
    const { container } = await show([])
    expect(container.querySelector(".bp-tokens")).toBeNull()
  })
})
