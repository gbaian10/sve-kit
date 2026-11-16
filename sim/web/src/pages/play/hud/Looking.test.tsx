import { fireEvent, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactNode } from "react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { Inspection } from "../../../battle/view/inspect"
import type { CardSummary } from "../../../data"
import { DEFAULT_PREFS, prefsStore } from "../../../settings"
import { renderRoutes } from "../../../test-utils"
import { type BattleCards, BattleCardsContext } from "../battle-cards"
import { SelectionBar } from "./SelectionBar"
import { type Shown, Sidebar } from "./Sidebar"
import { Zoom } from "./Zoom"

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  document.documentElement.removeAttribute("lang")
})

const FACE = { printing_id: "p:evolved", face: 0 }
const UNDER = { printing_id: "p:base", face: 0 }

/** A snapshot that knows two cards: an evolve card printed 5 cost 4/6 and the follower under it. */
const CARDS: BattleCards = {
  images: undefined,
  text: undefined,
  find: (card) => ({
    summary: {
      printingId: card.printing_id,
      faceId: "f:test",
      cardNo: card.printing_id === "p:base" ? "T-001" : "T-002",
      cost: 5,
      attack: 4,
      defense: 6,
      classCode: null,
      typeCode: "follower",
    } as CardSummary,
    name: { text: card.printing_id === "p:base" ? "測試從者" : "測試進化", lang: "zh-Hant" },
    original: undefined,
    untranslated: false,
    classLabel: "龍族",
    wording: undefined,
  }),
}

/** On the viewer's field: evolved, engaged, and hit down to 4/2. */
const STATE: Inspection = {
  card: {
    handle: "a1",
    card: UNDER,
    face: FACE,
    faceDown: false,
    acted: true,
    evolution: "evolved",
    stats: { attack: 4, life: 2 },
    keywords: [],
    counters: [],
    cost: null,
  },
  seat: "p1",
  zone: "field",
  under: UNDER,
  traits: [],
}

const SHOWN: Shown = { card: FACE, where: "你的場上", state: STATE, preview: false }

const show = (node: ReactNode) =>
  renderRoutes([
    { path: "*", element: <BattleCardsContext value={CARDS}>{node}</BattleCardsContext> },
  ])

describe("the info column", () => {
  it("shows a card's current numbers beside the printed ones, and where it lies", async () => {
    const onZoom = vi.fn()
    await show(
      <Sidebar shown={SHOWN} width={340} overlay={false} onZoom={onZoom} onClose={vi.fn()} />,
    )
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    expect(panel).toHaveTextContent("測試進化")
    expect(panel).toHaveTextContent("從者 · 已進化 · 橫置中")
    // Attack is what the card prints; its defense is down from 6 to 2.
    expect(panel).toHaveTextContent("攻擊 4印刷 4")
    expect(panel).toHaveTextContent("體力 2印刷 6")
    expect(panel).toHaveTextContent("進化前：測試從者（T-001）")
    expect(panel).toHaveTextContent("T-002 · 你的場上")
    expect(panel).not.toHaveTextContent("預覽")
    await userEvent.setup().click(screen.getByRole("button", { name: "查看大圖" }))
    expect(onZoom).toHaveBeenCalledOnce()
  })

  it("prints nothing beside numbers that are unchanged, and says when it is only a preview", async () => {
    const whole: Shown = {
      ...SHOWN,
      state: { ...STATE, card: { ...STATE.card, stats: { attack: 4, life: 6 }, acted: false } },
      preview: true,
    }
    await show(
      <Sidebar shown={whole} width={340} overlay={false} onZoom={vi.fn()} onClose={vi.fn()} />,
    )
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    expect(panel).not.toHaveTextContent("印刷")
    expect(panel).not.toHaveTextContent("橫置中")
    expect(panel).toHaveTextContent("預覽（滑鼠移開後回到選取的卡）")
  })

  it("shows a quoted cost that differs from the printed one", async () => {
    const quoted: Shown = {
      ...SHOWN,
      state: { ...STATE, card: { ...STATE.card, cost: 3 }, zone: "hand" },
    }
    await show(
      <Sidebar shown={quoted} width={340} overlay={false} onZoom={vi.fn()} onClose={vi.fn()} />,
    )
    expect(screen.getByRole("complementary", { name: "卡片資訊" })).toHaveTextContent(
      "費用 5現在 3",
    )
  })
})

describe("the large view", () => {
  it("names the card and closes from its button or with Escape", async () => {
    const onClose = vi.fn()
    await show(<Zoom zoomed={{ face: FACE, under: UNDER }} onClose={onClose} />)
    const dialog = screen.getByRole("dialog", { name: "測試進化" })
    expect(dialog).toHaveTextContent("T-002")
    expect(dialog).toHaveTextContent("從者 · 費用 5 · 4/6")
    expect(dialog).toHaveTextContent("進化前：測試從者（T-001）")
    await userEvent.setup().click(screen.getByRole("button", { name: "關閉" }))
    expect(onClose).toHaveBeenCalledOnce()
    fireEvent(dialog, new Event("cancel", { cancelable: true }))
    expect(onClose).toHaveBeenCalledTimes(2)
  })
})

describe("the selection bar", () => {
  it("sums the card up in a line with the way to its large view", async () => {
    const onZoom = vi.fn()
    const onClose = vi.fn()
    await show(<SelectionBar shown={SHOWN} onZoom={onZoom} onClose={onClose} />)
    const bar = screen.getByRole("region", { name: "選取的卡片" })
    // The numbers are the current ones: 4/2, not the printed 4/6.
    expect(bar).toHaveTextContent("測試進化從者 · 費用 5 · 4/2T-002 · 你的場上")
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "查看大圖" }))
    await user.click(screen.getByRole("button", { name: "關閉" }))
    expect(onZoom).toHaveBeenCalledOnce()
    expect(onClose).toHaveBeenCalledOnce()
  })
})
