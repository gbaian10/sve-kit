import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { StrictMode } from "react"
import type { RouteObject } from "react-router"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { HostMessage, Seat } from "../../battle/contract"
import { cardRef } from "../../battle/fixtures/cards"
import type { BattleSource } from "../../battle/source"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { BattleScreen } from "./BattleScreen"
import { PlayPage } from "./PlayPage"

const ROUTES: RouteObject[] = [{ path: "/play", Component: PlayPage }]
const DESKTOP = { width: window.innerWidth, height: window.innerHeight }

function resize(width: number, height: number) {
  act(() => {
    Object.assign(window, { innerWidth: width, innerHeight: height })
    window.dispatchEvent(new Event("resize"))
  })
}

beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  document.documentElement.removeAttribute("lang")
  Object.assign(window, { innerWidth: DESKTOP.width, innerHeight: DESKTOP.height })
  vi.unstubAllEnvs()
  vi.resetModules()
})

const region = (view: HTMLElement, name: string): HTMLElement =>
  within(view).getByRole("region", { name })

const table = async (): Promise<HTMLElement> => screen.findByRole("region", { name: "對戰桌" })

describe("PlayPage", () => {
  it("shows the viewer's table with the other hand as backs only", async () => {
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await table()
    expect(
      within(region(container, "對手的手牌")).getAllByRole("img", { name: "卡背" }),
    ).toHaveLength(5)
    const own = region(container, "你的手牌")
    expect(within(own).queryByRole("img", { name: "卡背" })).toBeNull()
    expect(own.querySelectorAll(".bp-hand-card")).toHaveLength(6)
    expect(region(container, "你")).toHaveTextContent("後攻")
    expect(region(container, "對手")).toHaveTextContent("先攻")
    expect(screen.getByText("你的主要階段")).toBeInTheDocument()
    expect(screen.getByText("第 5 回合")).toBeInTheDocument()
    // Nothing on the table can be answered yet.
    expect(screen.getByRole("button", { name: "回合結束" })).toBeDisabled()
    // Super evolution opens on the seat's next turn: greyed, with the count on it.
    const ownMat = within(await table()).getByRole("region", { name: "你的遊戲墊" })
    expect(within(ownMat).getByRole("img", { name: "超進化還有 1 回合" })).toHaveClass("is-locked")
    expect(region(container, "你")).toHaveTextContent("2/3")
    expect(region(container, "你").querySelector(".bp-hud-sep")).toHaveClass("is-locked")
    expect(region(container, "對手").querySelector(".bp-hud-ep")).toHaveClass("is-none")
  })

  it("draws each pile with its size and each player's points", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mid"] })
    const own = within(await table()).getByRole("region", { name: "你的遊戲墊" })
    expect(within(own).getByRole("group", { name: "牌庫" })).toHaveTextContent("牌庫22")
    expect(within(own).getByRole("group", { name: "進化牌庫" })).toHaveTextContent(
      "進化牌庫8已用 1",
    )
    expect(within(own).getByRole("group", { name: "墓場" })).toHaveTextContent("墓場4")
    expect(within(own).getByRole("group", { name: "主戰者" })).toHaveTextContent("16")
    // The first player has no evolve points; the other seat has used one of its three.
    expect(within(own).getByRole("img", { name: "EP 0/0" })).toHaveTextContent("無（先攻）")
    expect(within(own).getByRole("img", { name: "超進化還有 2 回合" })).toHaveClass("is-locked")
    const other = within(await table()).getByRole("region", { name: "對手的遊戲墊" })
    const spent = within(other).getByRole("img", { name: "EP 2/3" })
    expect(spent.querySelectorAll(".bp-ep-icon")).toHaveLength(3)
    expect(spent.querySelectorAll(".bp-ep-icon.is-used")).toHaveLength(1)
    // Each PP column is lit from 0 up to its value: six of the maximum, two of what is left.
    const pp = within(own).getByRole("img", { name: "PP 剩 1，上限 5" })
    expect(pp.querySelectorAll(".bp-pp-dot.is-max.is-on")).toHaveLength(6)
    expect(pp.querySelectorAll(".bp-pp-dot.is-left.is-on")).toHaveLength(2)
    expect(pp.querySelector(".bp-pp-dot.is-left.is-mark")).toHaveTextContent("1")
  })

  it("keeps a spent super evolution point on the mat as a faded marker", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=full"] })
    const own = within(await table()).getByRole("region", { name: "你的遊戲墊" })
    const spent = within(own).getByRole("img", { name: "SEP 0/1" })
    expect(spent).not.toHaveClass("is-locked")
    expect(spent.querySelectorAll(".bp-sep-icon.is-used")).toHaveLength(1)
    const other = within(await table()).getByRole("region", { name: "對手的遊戲墊" })
    const held = within(other).getByRole("img", { name: "SEP 1/1" })
    expect(held.querySelectorAll(".bp-sep-icon.is-used")).toHaveLength(0)
  })

  it("draws what the authority reports for the cards in play", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mid"] })
    const board = await table()
    const own = within(board).getByRole("region", { name: "你的遊戲墊" })
    const other = within(board).getByRole("region", { name: "對手的遊戲墊" })
    // An evolved follower lies on its evolve card; an engaged ward lights its die.
    expect(within(own).getByText("進化")).toBeInTheDocument()
    expect(own.querySelector(".bp-kwdie.is-on")).toHaveTextContent("守護")
    expect(own.querySelectorAll(".bp-card.is-engaged")).toHaveLength(2)
    expect(within(own).getByText("スタック 2")).toBeInTheDocument()
    expect(within(other).getByText("超進化")).toBeInTheDocument()
    // A keyword without a die of its own is written out.
    expect(within(other).getByText("疾馳")).toHaveClass("bp-status")
    expect(within(own).getAllByRole("img", { name: /^攻擊 \d+/ }).length).toBeGreaterThan(0)
  })

  it("gives each seat its own screen in the dual view", async () => {
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play?dual=1"] })
    const pane = await screen.findByRole("region", { name: "對手看到的畫面（只能看）" })
    await within(pane).findByRole("region", { name: "對戰桌" })
    expect(container.querySelectorAll(".bp-stage")).toHaveLength(2)
    // The same match from both chairs: each seat's own main phase reads differently.
    expect(within(pane).getByText("對手的主要階段")).toBeInTheDocument()
    expect(screen.getAllByText("你的主要階段")).toHaveLength(1)
    // The other seat sees its own five cards and six backs for the viewer's hand.
    expect(region(pane, "你的手牌").querySelectorAll(".bp-dual-card")).toHaveLength(5)
    expect(within(region(pane, "對手的手牌")).getAllByRole("img", { name: "卡背" })).toHaveLength(6)
  })

  it("still shows the other seat when React runs the page's effects twice", async () => {
    const strict: RouteObject[] = [
      {
        path: "/play",
        element: (
          <StrictMode>
            <PlayPage />
          </StrictMode>
        ),
      },
    ]
    await renderRoutes(strict, { initialEntries: ["/play?dual=1"] })
    const pane = await screen.findByRole("region", { name: "對手看到的畫面（只能看）" })
    expect(await within(pane).findByRole("region", { name: "對戰桌" })).toBeInTheDocument()
  })

  it("switches scene from the picker and keeps it in the address", async () => {
    const user = userEvent.setup()
    const { router, container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await screen.findByText("第 5 回合")
    await user.selectOptions(screen.getByRole("combobox", { name: "場景" }), "開局")
    await waitFor(() => {
      expect(container.querySelector(".bp-turn-no")).toHaveTextContent("開局")
    })
    expect(router.state.location.search).toBe("?scene=opening")
    expect(
      within(region(container, "對手的手牌")).getAllByRole("img", { name: "卡背" }),
    ).toHaveLength(4)
  })

  it("turns the dual view on and off from its buttons", async () => {
    const user = userEvent.setup()
    const { router } = await renderRoutes(ROUTES, { initialEntries: ["/play?scene=full"] })
    await table()
    await user.click(screen.getByRole("button", { name: "雙視角" }))
    expect(
      await screen.findByRole("region", { name: "對手看到的畫面（只能看）" }),
    ).toBeInTheDocument()
    expect(router.state.location.search).toBe("?scene=full&dual=1")
    await user.click(screen.getByRole("button", { name: "離開雙視角" }))
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "對手看到的畫面（只能看）" })).toBeNull()
    })
    expect(router.state.location.search).toBe("?scene=full")
  })

  it("draws one shared mat or one per player, as the battle setting says", async () => {
    const user = userEvent.setup()
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await table()
    const count = (selector: string) => container.querySelectorAll(selector).length
    // One picture per mat; then one picture under both sides, which keep only their zone frames.
    expect([count(".bp-mat"), count(".bp-mat-art"), count(".bp-mat.is-frames")]).toEqual([2, 2, 0])
    await user.click(screen.getByRole("button", { name: "對戰設定" }))
    await user.click(screen.getByRole("radio", { name: "共用一張" }))
    expect(prefsStore.get().battleMat).toBe("shared")
    expect([count(".bp-mat"), count(".bp-mat-art"), count(".bp-mat.is-frames")]).toEqual([3, 1, 2])
    expect(count(".bp-frames")).toBe(2)
    // The page's background is not the mat: it stays as it was.
    expect(count(".bp-leader-art")).toBe(1)
  })

  it("moves the piles into a strip on a phone held upright", async () => {
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await table()
    expect(container.querySelector(".bp-pp-bar")).toBeNull()
    resize(390, 844)
    expect(container.querySelector(".bp-root")).toHaveClass("is-stacked", "plates-top")
    expect(container.querySelectorAll(".bp-pp-bar")).toHaveLength(2)
    expect(container.querySelector(".bp-pp")).toBeNull()
    // The same piles, smaller: their sizes stay on them.
    expect(screen.getAllByRole("group", { name: "牌庫" })[0]).toHaveTextContent("23")
  })

  it("opens the card-info sidebar from its tab where it floats over the board", async () => {
    const user = userEvent.setup()
    await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await table()
    resize(1180, 820)
    expect(screen.queryByRole("complementary", { name: "卡片資訊" })).toBeNull()
    await user.click(screen.getByRole("button", { name: "展開卡片資訊" }))
    expect(screen.getByRole("complementary", { name: "卡片資訊" })).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "收合卡片資訊" }))
    expect(screen.queryByRole("complementary", { name: "卡片資訊" })).toBeNull()
  })
})

function source(seat: Seat, messages: readonly HostMessage[]): BattleSource {
  return {
    seat,
    listen(listener) {
      for (const message of messages) listener(message)
      return () => undefined
    },
    close: () => undefined,
  }
}

describe("BattleScreen", () => {
  const leaders = { p1: cardRef("BP10-LD05"), p2: cardRef("BP05-LD02") }
  const routes = (messages: readonly HostMessage[]): RouteObject[] => [
    {
      path: "*",
      element: <BattleScreen source={source("p1", messages)} leaders={leaders} />,
    },
  ]

  it("waits while the host has sent no table yet", async () => {
    await renderRoutes(routes([]))
    expect(await screen.findByRole("status")).toHaveTextContent("正在載入對局")
  })

  it("stops for good when the host reports an error", async () => {
    await renderRoutes(routes([{ kind: "host_error", format: 1, reason: "projection" }]))
    expect(await screen.findByRole("alert")).toHaveTextContent("無法繼續")
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "對戰桌" })).toBeNull()
    })
  })
})

describe("the /play route", () => {
  async function tableWith(flag: string | undefined) {
    vi.resetModules()
    vi.unstubAllEnvs()
    if (flag !== undefined) vi.stubEnv("VITE_DEV_ONLY_PLAY", flag)
    const { routes } = await import("../../app/routes")
    return routes
  }

  it("exists only in builds that turn the dev-only switch on", async () => {
    const paths = (routeTable: RouteObject[]) => routeTable.map((route) => route.path)
    expect(paths(await tableWith("1"))).toContain("/play")
    expect(paths(await tableWith(undefined))).not.toContain("/play")
    expect(paths(await tableWith("0"))).not.toContain("/play")
  })
})
