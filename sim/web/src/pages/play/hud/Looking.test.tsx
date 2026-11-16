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
  search: () => [],
  find: (card) => ({
    summary: {
      printingId: card.printing_id,
      faceId: "f:test",
      cardNo: card.printing_id === "p:base" ? "T-001" : "T-002",
      cost: 5,
      attack: card.printing_id === "p:amulet" ? null : 4,
      defense: card.printing_id === "p:amulet" ? null : 6,
      classCode: null,
      typeCode: card.printing_id === "p:amulet" ? "amulet" : "follower",
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
    attack: null,
  },
  seat: "p1",
  zone: "field",
  under: UNDER,
  type: "フォロワー・エボルヴ",
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
      <Sidebar
        shown={SHOWN}
        width={340}
        overlay={false}
        pinned={false}
        onPin={vi.fn()}
        onZoom={onZoom}
        onClose={vi.fn()}
      />,
    )
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    expect(panel).toHaveTextContent("測試進化")
    expect(panel).toHaveTextContent("從者 · 已進化 · 橫置中")
    // Attack is what the card prints; its defense is down from 6 to 2.
    // On the field nothing quotes a cost: the 5 is what the card prints, and says so.
    expect(panel).toHaveTextContent("費用 5印刷值")
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
      <Sidebar
        shown={whole}
        width={340}
        overlay={false}
        pinned={false}
        onPin={vi.fn()}
        onZoom={vi.fn()}
        onClose={vi.fn()}
      />,
    )
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    // Nothing printed beside the numbers; only the cost, which the field does not quote.
    expect(panel).toHaveTextContent("攻擊 4體力 6")
    expect(panel).not.toHaveTextContent("印刷 ")
    expect(panel).not.toHaveTextContent("橫置中")
    expect(panel).toHaveTextContent("預覽（滑鼠移開後回到選取的卡）")
  })

  it("says whether it is pinned and hands the pin's press back", async () => {
    const onPin = vi.fn()
    await show(
      <Sidebar
        shown={SHOWN}
        width={340}
        overlay={false}
        pinned={false}
        onPin={onPin}
        onZoom={vi.fn()}
        onClose={vi.fn()}
      />,
    )
    const pin = screen.getByRole("button", { name: "釘選（F1）" })
    expect(pin).toHaveAttribute("aria-pressed", "false")
    await userEvent.setup().click(pin)
    expect(onPin).toHaveBeenCalledOnce()
  })

  it("shows itself pinned", async () => {
    await show(
      <Sidebar
        shown={SHOWN}
        width={340}
        overlay={false}
        pinned
        onPin={vi.fn()}
        onZoom={vi.fn()}
        onClose={vi.fn()}
      />,
    )
    const pin = screen.getByRole("button", { name: "取消釘選（F1）" })
    expect(pin).toHaveAttribute("aria-pressed", "true")
    expect(pin).toHaveClass("is-on")
  })

  it("lists the counters on the card by kind", async () => {
    const holding: Shown = {
      ...SHOWN,
      state: {
        ...STATE,
        card: {
          ...STATE.card,
          counters: [
            { name: "スタックカウンター", amount: 2 },
            { name: "融合カウンター", amount: 1 },
          ],
        },
      },
    }
    await show(
      <Sidebar
        shown={holding}
        width={340}
        overlay={false}
        pinned={false}
        onPin={vi.fn()}
        onZoom={vi.fn()}
        onClose={vi.fn()}
      />,
    )
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    const kinds = Array.from(
      panel.querySelectorAll(".bp-detail-list span"),
      (kind) => kind.textContent,
    )
    expect(kinds).toEqual(["蓄積 ×2", "融合 ×1"])
    expect(panel).not.toHaveTextContent("スタック")
  })

  const sidebar = (shown: Shown) =>
    show(
      <Sidebar
        shown={shown}
        width={340}
        overlay={false}
        pinned={false}
        onPin={vi.fn()}
        onZoom={vi.fn()}
        onClose={vi.fn()}
      />,
    )
  const HAND: Inspection = {
    ...STATE,
    zone: "hand",
    under: null,
    card: { ...STATE.card, stats: null, acted: false, evolution: "base" },
  }

  it("shows what the authority quotes for a card in hand, with the printed cost beside it", async () => {
    await sidebar({ ...SHOWN, state: { ...HAND, card: { ...HAND.card, cost: 3 } } })
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    // The chip is the quote; the printed 5 is said to be printed.
    expect(panel).toHaveTextContent("費用 3印刷 5")
    // In hand the authority reports no numbers: the card's own are said to be printed.
    expect(panel).toHaveTextContent("攻擊 4體力 6印刷值")
  })

  it("says nothing beside a quote that is the printed cost", async () => {
    await sidebar({ ...SHOWN, state: { ...HAND, card: { ...HAND.card, cost: 5 } } })
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    expect(panel).toHaveTextContent("費用 5攻擊")
  })

  it("says a cost is unknown where the authority quotes none, never the printed one in its place", async () => {
    await sidebar({ ...SHOWN, state: HAND })
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    expect(panel).toHaveTextContent("費用 ?費用未定印刷 5")
  })

  it("shows the kind a card has now: a follower that became an amulet has no numbers of its own", async () => {
    await sidebar({
      ...SHOWN,
      state: { ...STATE, type: "アミュレット", card: { ...STATE.card, stats: null } },
    })
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    expect(panel).toHaveTextContent("護符（印刷：從者） · 已進化")
    // No current numbers: the 4 and 6 are what the card prints, and say so.
    expect(panel).toHaveTextContent("攻擊 4體力 6印刷值")
    expect(panel).not.toHaveTextContent("印刷 4")
  })

  it("shows the numbers of a card that became a follower, though it prints none", async () => {
    await sidebar({
      card: { printing_id: "p:amulet", face: 0 },
      where: "你的場上",
      preview: false,
      state: {
        ...STATE,
        under: null,
        type: "フォロワー",
        card: { ...STATE.card, evolution: "base", acted: false, stats: { attack: 2, life: 3 } },
      },
    })
    const panel = screen.getByRole("complementary", { name: "卡片資訊" })
    expect(panel).toHaveTextContent("從者（印刷：護符）")
    expect(panel).toHaveTextContent("攻擊 2體力 3")
    expect(panel).not.toHaveTextContent("體力 3印刷")
  })

  it("writes a type it has no word for as the authority names it", async () => {
    await sidebar({ ...SHOWN, state: { ...STATE, type: "トークン" } })
    const named = screen.getByText("トークン")
    expect(named).toHaveAttribute("lang", "ja")
    expect(screen.getByRole("complementary", { name: "卡片資訊" })).not.toHaveTextContent(
      "（印刷：",
    )
  })
})

describe("the large view", () => {
  it("names the card and closes from its button or with Escape", async () => {
    const onClose = vi.fn()
    await show(<Zoom zoomed={{ face: FACE, under: UNDER }} onClose={onClose} />)
    const dialog = screen.getByRole("dialog", { name: "測試進化" })
    expect(dialog).toHaveTextContent("T-002")
    // The large view shows a face, not a card in play: everything it says is what the face prints.
    expect(dialog).toHaveTextContent("從者 · 印刷費用 5 · 印刷 4/6")
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
    // The numbers are the current ones: 4/2, not the printed 4/6. The field quotes no cost.
    expect(bar).toHaveTextContent("測試進化從者 · 印刷費用 5 · 4/2T-002 · 你的場上")
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "查看大圖" }))
    await user.click(screen.getByRole("button", { name: "關閉" }))
    expect(onZoom).toHaveBeenCalledOnce()
    expect(onClose).toHaveBeenCalledOnce()
  })

  const HAND: Inspection = {
    ...STATE,
    zone: "hand",
    under: null,
    type: "フォロワー",
    card: { ...STATE.card, stats: null, acted: false, evolution: "base" },
  }
  const bar = async (state: Inspection) => {
    await show(
      <SelectionBar
        shown={{ ...SHOWN, where: "你的手牌", state }}
        onZoom={vi.fn()}
        onClose={vi.fn()}
      />,
    )
    return screen.getByRole("region", { name: "選取的卡片" })
  }

  it("shows the quoted cost of a card in hand, and the printed one as printed", async () => {
    expect(await bar({ ...HAND, card: { ...HAND.card, cost: 3 } })).toHaveTextContent(
      "從者 · 費用 3（印刷 5） · 印刷 4/6",
    )
  })

  it("says a cost is unknown where the authority quotes none", async () => {
    expect(await bar(HAND)).toHaveTextContent("從者 · 費用未定（印刷 5） · 印刷 4/6")
  })

  it("names the kind a card has now", async () => {
    expect(
      await bar({ ...STATE, type: "アミュレット", card: { ...STATE.card, stats: null } }),
    ).toHaveTextContent("護符（印刷：從者） · 印刷費用 5 · 印刷 4/6")
  })
})
