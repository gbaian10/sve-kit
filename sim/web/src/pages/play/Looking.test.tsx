import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { RouteObject } from "react-router"
import { afterEach, beforeEach, describe, expect, it } from "vitest"

import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { PlayPage } from "./PlayPage"

const ROUTES: RouteObject[] = [{ path: "/play", Component: PlayPage }]

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

const WINDOW = { innerWidth: window.innerWidth, innerHeight: window.innerHeight }

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  document.documentElement.removeAttribute("lang")
  Object.assign(window, WINDOW)
})

const mat = async (name: string): Promise<HTMLElement> =>
  within(await screen.findByRole("region", { name: "對戰桌" })).getByRole("region", { name })

const pickable = (within_: HTMLElement): HTMLElement[] =>
  Array.from(within_.querySelectorAll<HTMLElement>('.bp-card[role="button"]'))

describe("looking at cards", () => {
  it("marks the card that was picked, by click or from the keyboard", async () => {
    const user = userEvent.setup()
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mid"] })
    const [first, second] = pickable(await mat("你的遊戲墊"))
    if (first === undefined || second === undefined) throw new Error("no cards to pick")
    expect(first).toHaveAttribute("aria-pressed", "false")
    await user.click(first)
    expect(first).toHaveClass("is-selected")
    expect(first).toHaveAttribute("aria-pressed", "true")
    second.focus()
    await user.keyboard("{Enter}")
    expect(second).toHaveClass("is-selected")
    expect(first).not.toHaveClass("is-selected")
  })

  it("picks a hand card and a leader the same way", async () => {
    const user = userEvent.setup()
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mid"] })
    const own = await mat("你的遊戲墊")
    const inHand = container.querySelector<HTMLElement>('.bp-hand-card[role="button"]')
    const leader = own.querySelector<HTMLElement>('.bp-pile-hit[role="button"]')
    if (inHand === null || leader === null) throw new Error("nothing to pick")
    await user.click(inHand)
    expect(inHand).toHaveClass("is-selected")
    await user.click(leader)
    expect(leader).toHaveClass("is-selected")
    expect(inHand).not.toHaveClass("is-selected")
  })

  it("opens a pile's cards and closes them again", async () => {
    const user = userEvent.setup()
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mid"] })
    const own = await mat("你的遊戲墊")
    await user.click(within(own).getByRole("button", { name: "查看墓場" }))
    const dialog = screen.getByRole("dialog", { name: "你的墓場（4 張）" })
    expect(dialog.querySelectorAll(".bp-browser-grid button")).toHaveLength(4)
    await user.click(within(dialog).getByRole("button", { name: "關閉" }))
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull()
    })
    // Nobody may look through a deck: it has nothing to open, only a note of its size.
    expect(within(own).queryByRole("button", { name: "查看牌庫" })).toBeNull()
    expect(within(own).getByRole("group", { name: "牌庫" })).toHaveAttribute(
      "title",
      "你的牌庫 22 張（不公開）",
    )
  })

  it("shows an evolve deck's face-down cards to their owner only", async () => {
    const user = userEvent.setup()
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mid"] })
    await user.click(within(await mat("你的遊戲墊")).getByRole("button", { name: "查看進化牌庫" }))
    const own = screen.getByRole("dialog", { name: "你的進化牌庫（背面 8 張 · 已公開 1 張）" })
    expect(within(own).getByText("背面 8 張（只有你看得到）")).toBeInTheDocument()
    expect(own.querySelectorAll(".bp-browser-grid button.is-secret")).toHaveLength(8)
    expect(own.querySelectorAll(".bp-browser-grid button")).toHaveLength(9)
    await user.click(within(own).getByRole("button", { name: "關閉" }))

    await user.click(
      within(await mat("對手的遊戲墊")).getByRole("button", { name: "查看進化牌庫" }),
    )
    const other = screen.getByRole("dialog", { name: "對手的進化牌庫（背面 8 張 · 已公開 1 張）" })
    expect(within(other).getByText("背面 8 張（不公開）")).toBeInTheDocument()
    // Only the used card, lying face up, can be looked at.
    expect(other.querySelectorAll(".bp-browser-grid button")).toHaveLength(1)
    expect(other.querySelectorAll(".is-secret")).toHaveLength(0)
  })

  it("pins the info column from its button or with F1, and remembers it", async () => {
    const user = userEvent.setup()
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mid"] })
    await mat("你的遊戲墊")
    // Wide enough for the info column to stand open beside the board.
    act(() => {
      Object.assign(window, { innerWidth: 1600, innerHeight: 900 })
      window.dispatchEvent(new Event("resize"))
    })
    const pin = screen.getByRole("button", { name: "釘選（F1）" })
    await user.click(pin)
    expect(prefsStore.get().battleInfoPinned).toBe(true)
    expect(screen.getByRole("button", { name: "取消釘選（F1）" })).toHaveAttribute(
      "aria-pressed",
      "true",
    )
    await user.keyboard("{F1}")
    expect(prefsStore.get().battleInfoPinned).toBe(false)
    expect(screen.getByRole("button", { name: "釘選（F1）" })).toBeInTheDocument()
  })

  it("lets nothing be picked on the other seat's half of the dual view", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mid&dual=1"] })
    const other = await screen.findByRole("region", { name: "對手看到的畫面（只能看）" })
    await waitFor(() => {
      expect(other.querySelector(".bp-card")).not.toBeNull()
    })
    expect(other.querySelectorAll('[role="button"]')).toHaveLength(0)
    expect(other.querySelectorAll(".bp-pile-hit")).toHaveLength(0)
    // The viewer's own half still is.
    expect(pickable(document.body).length).toBeGreaterThan(0)
  })
})
