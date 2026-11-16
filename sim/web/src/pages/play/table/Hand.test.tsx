import { screen } from "@testing-library/react"
import { beforeEach, describe, expect, it } from "vitest"

import type { TableCard } from "../../../battle/view/table"
import type { CardSummary } from "../../../data"
import { prefsStore } from "../../../settings"
import { renderRoutes } from "../../../test-utils"
import { type BattleCards, BattleCardsContext } from "../battle-cards"
import { computeLayout } from "../layout"
import { Hand } from "./Hand"

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

/** A snapshot that knows the card: printed cost 5, a 4/6 follower. */
const CARDS: BattleCards = {
  images: undefined,
  find: () => ({
    summary: {
      printingId: "p:test",
      faceId: "f:test",
      cardNo: "T-001",
      cost: 5,
      attack: 4,
      defense: 6,
      classCode: null,
    } as CardSummary,
    name: { text: "測試從者", lang: "zh-Hant" },
    original: undefined,
    untranslated: false,
    classLabel: null,
  }),
}

const inHand = (cost: number | null): TableCard => ({
  handle: "a1",
  card: { printing_id: "p:test", face: 0 },
  face: { printing_id: "p:test", face: 0 },
  faceDown: true,
  acted: false,
  evolution: "base",
  stats: null,
  keywords: [],
  counters: [],
  cost,
})

const show = (cost: number | null) =>
  renderRoutes([
    {
      path: "*",
      element: (
        <BattleCardsContext value={CARDS}>
          <Hand
            hand={{ count: 1, cards: [inHand(cost)] }}
            layout={computeLayout(1280, 800, { sidebar: true, dual: false })}
          />
        </BattleCardsContext>
      ),
    },
  ])

describe("Hand", () => {
  it("shows the cost the authority quotes", async () => {
    const { container } = await show(3)
    await screen.findByRole("region", { name: "你的手牌" })
    expect(container.querySelector(".bp-cost")).toHaveTextContent("費用3")
  })

  it("says a cost is unknown instead of showing the printed one", async () => {
    const { container } = await show(null)
    await screen.findByRole("region", { name: "你的手牌" })
    const cost = container.querySelector(".bp-cost")
    expect(cost).toHaveTextContent("費用未定?")
    expect(cost).not.toHaveTextContent("5")
  })
})
