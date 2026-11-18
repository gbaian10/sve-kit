import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { StrictMode } from "react"
import type { RouteObject } from "react-router"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { HostMessage, Seat } from "../../battle/contract"
import { cardRef } from "../../battle/fixtures/cards"
import { SCENES } from "../../battle/fixtures/scenes"
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

  it("lays a counter on its card as a chip with the number, named in the reader's language", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=stack"] })
    const own = within(await table()).getByRole("region", { name: "你的遊戲墊" })
    const chip = within(own).getByRole("img", { name: "指示物：蓄積 ×3" })
    expect(chip).toHaveAttribute("title", "蓄積 ×3")
    // The chip carries the number only: no kind is printed on it, in any language.
    expect(chip).toHaveTextContent(/^3$/)
    expect(chip.querySelector("img")).toHaveAttribute("src", expect.stringMatching(/chip_b\.webp$/))
    // An engaged card carries its chip too.
    const engaged = within(own).getByRole("img", { name: "指示物：蓄積 ×1" })
    expect(engaged.closest(".bp-card")).toHaveClass("is-engaged")
    prefsStore.set({ uiLanguage: "en" })
    expect(await within(own).findByRole("img", { name: "Counter: Stack ×3" })).toBeInTheDocument()
  })

  it("stacks a card's keyword markers, standing and engaged", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=stack"] })
    const own = within(await table()).getByRole("region", { name: "你的遊戲墊" })
    const rows = Array.from(own.querySelectorAll(".bp-status-row")).filter(
      (row) => row.querySelectorAll(".bp-kwdie").length === 3,
    )
    expect(rows).toHaveLength(2)
    expect(rows.map((row) => row.closest(".bp-card")?.classList.contains("is-engaged"))).toEqual([
      false,
      true,
    ])
    expect(rows[0]).toHaveTextContent("守護威懾必殺")
  })

  it("shows the current numbers of a follower waiting in the EX area", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=stack"] })
    const own = within(await table()).getByRole("region", { name: "你的遊戲墊" })
    // Nothing on the field shows these numbers: they are the EX follower's, given like a field one's.
    expect(within(own).getByRole("img", { name: /^攻擊 4/ })).toBeInTheDocument()
    expect(within(own).getByRole("img", { name: /^體力 5/ })).toBeInTheDocument()
  })

  it("lights the cards that may attack: green for the leader, yellow for followers only", async () => {
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    const own = within(await table()).getByRole("region", { name: "你的遊戲墊" })
    expect(own.querySelectorAll(".bp-card.is-attack-leader")).toHaveLength(1)
    expect(own.querySelectorAll(".bp-card.is-attack-followers")).toHaveLength(1)
    // The engaged card may not attack, and nothing is said about the other seat's cards.
    expect(own.querySelector(".bp-card.is-engaged")).not.toHaveClass("is-attack-leader")
    const other = within(await table()).getByRole("region", { name: "對手的遊戲墊" })
    expect(other.querySelector(".is-attack-leader, .is-attack-followers")).toBeNull()
    // Storm and rush have no marker of their own on a card.
    expect(container.querySelector(".bp-status")).toBeNull()
  })

  it("lights what the picked card may attack, as the scene says and for as long as it is picked", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    const own = within(await table()).getByRole("region", { name: "你的遊戲墊" })
    const other = within(await table()).getByRole("region", { name: "對手的遊戲墊" })
    expect(other.querySelector(".is-target")).toBeNull()
    const [green] = own.querySelectorAll(".bp-card.is-attack-leader")
    const [yellow] = own.querySelectorAll(".bp-card.is-attack-followers")
    if (green === undefined || yellow === undefined) throw new Error("no attackers")
    // The one that reaches the leader: the leader and the one follower the scene names.
    fireEvent.click(green)
    expect(other.querySelectorAll(".bp-card.is-target")).toHaveLength(1)
    expect(other.querySelector(".bp-card.is-target")).toHaveClass("is-engaged")
    expect(other.querySelectorAll(".bp-pile-hit.is-target")).toHaveLength(1)
    // The one that reaches followers only: the leader goes dark.
    fireEvent.click(yellow)
    expect(other.querySelectorAll(".bp-card.is-target")).toHaveLength(1)
    expect(other.querySelector(".bp-pile-hit.is-target")).toBeNull()
    // A card that may not attack lights nothing.
    const engaged = own.querySelector(".bp-card.is-engaged")
    if (engaged === null) throw new Error("no engaged card")
    fireEvent.click(engaged)
    expect(other.querySelector(".is-target")).toBeNull()
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
    expect(within(other).getByText("超進化")).toBeInTheDocument()
    // Storm has no marker on the card: what it allows shows as the frame when the card may attack.
    expect(within(other).queryByText("疾馳")).toBeNull()
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

  it.each(SCENES.map((scene) => scene.id))("opens the %s scene", async (id) => {
    await renderRoutes(ROUTES, { initialEntries: [`/play?scene=${id}`] })
    // Whatever the scene asks, the table stands; only a host error replaces it with a notice.
    if (id === "halted") expect(await screen.findByRole("alert")).toBeInTheDocument()
    else expect(await table()).toBeInTheDocument()
  })

  it("calls the opening by its name while no turn has begun, whoever is the active seat", async () => {
    await renderRoutes(ROUTES, { initialEntries: ["/play?scene=mulligan"] })
    await table()
    // The first player is chosen, so a seat is active; the opening still has no turn to number.
    const turn = screen.getByRole("region", { name: "回合" })
    expect(within(turn).getByText("開局")).toBeInTheDocument()
    expect(within(turn).queryByText(/第 \d+ 回合/)).toBeNull()
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

  it("lays one picture under both mats: across the whole table, or on the board only", async () => {
    const user = userEvent.setup()
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await table()
    const count = (selector: string) => container.querySelectorAll(selector).length
    const art = () => {
      const found = container.querySelector<HTMLElement>(".bp-mat-art")
      if (found === null) throw new Error("no mat picture")
      return found
    }
    // One picture for the table; the two mats carry their printing only.
    expect([count(".bp-mat-art"), count(".bp-mat"), count(".bp-frames")]).toEqual([1, 2, 2])
    expect(art()).not.toHaveClass("is-framed")
    const across = Number.parseFloat(art().style.width)
    await user.click(screen.getByRole("button", { name: "對戰設定" }))
    await user.click(screen.getByRole("radio", { name: "盤面" }))
    expect(prefsStore.get().battleMatReach).toBe("board")
    // Still one picture, now framed like a mat and no wider than the board.
    expect(count(".bp-mat-art")).toBe(1)
    expect(art()).toHaveClass("is-framed")
    expect(Number.parseFloat(art().style.width)).toBeLessThan(across)
  })

  it("changes the mat's picture from the battle settings, keeping the printing", async () => {
    const user = userEvent.setup()
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await table()
    expect(container.querySelectorAll(".bp-night-rune")).toHaveLength(1)
    await user.click(screen.getByRole("button", { name: "對戰設定" }))
    await user.selectOptions(screen.getByRole("combobox", { name: "遊戲墊圖案" }), "羊皮紙")
    expect(prefsStore.get().battleMatLook).toBe("parchment")
    expect(container.querySelectorAll(".bp-parchment-ring")).toHaveLength(1)
    expect(container.querySelector(".bp-night-rune")).toBeNull()
    // A light mat prints in a dark ink; the printing itself is the same.
    expect(container.querySelectorAll(".bp-mat.is-light")).toHaveLength(2)
    expect(container.querySelectorAll(".bp-zone-base")).toHaveLength(10)
  })

  it("shows a card's art on the mat, blurred and darkened as the sliders say", async () => {
    const user = userEvent.setup()
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await table()
    await user.click(screen.getByRole("button", { name: "對戰設定" }))
    // The sliders belong to a photo: a drawn mat has none.
    expect(screen.queryByRole("slider", { name: "模糊" })).toBeNull()
    await user.selectOptions(screen.getByRole("combobox", { name: "遊戲墊圖案" }), "卡面")
    const photo = () => {
      const found = container.querySelector<HTMLElement>(".bp-mat-art .bp-mat-photo")
      if (found === null) throw new Error("no photo on the mat")
      return found
    }
    // Until a card is picked the art is the player's own leader's.
    expect(prefsStore.get().battleMatCard).toBeNull()
    expect(photo().querySelector(".bp-art")).not.toBeNull()
    // As it starts: no blur at all (the art as it is, nothing overhanging its box), and darkened.
    expect([photo().style.filter, photo().style.inset]).toEqual(["", "0px"])
    expect(screen.getByText("不模糊")).toBeInTheDocument()
    expect(container.querySelector<HTMLElement>(".bp-mat-veil")?.style.opacity).toBe("0.6")
    const blur = screen.getByRole("slider", { name: "模糊" })
    // A little blur is all there is: more only smears the picture.
    expect(blur).toHaveAttribute("max", "5")
    fireEvent.change(blur, { target: { value: "3" } })
    expect(prefsStore.get().battleMatBlur).toBe(3)
    expect([photo().style.filter, photo().style.inset]).toEqual(["blur(3px)", "-6px"])
    fireEvent.change(screen.getByRole("slider", { name: "暗化" }), { target: { value: "20" } })
    expect(container.querySelector<HTMLElement>(".bp-mat-veil")?.style.opacity).toBe("0.2")
  })

  it("falls back to the plain mat while the player has uploaded no picture", async () => {
    prefsStore.set({ battleMatLook: "upload" })
    const user = userEvent.setup()
    const { container } = await renderRoutes(ROUTES, { initialEntries: ["/play"] })
    await table()
    expect(container.querySelector(".bp-mat-art .bp-mat-plain")).not.toBeNull()
    expect(container.querySelector(".bp-mat-photo")).toBeNull()
    await user.click(screen.getByRole("button", { name: "對戰設定" }))
    expect(screen.getByText("還沒有圖片")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "選擇圖片" })).toBeInTheDocument()
    // Nothing of this is shared: the settings say so.
    expect(screen.getByText("這些只存在這台裝置，對手看不到你的遊戲墊。")).toBeInTheDocument()
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
    submit: () => Promise.reject(new Error("nothing is answered here")),
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

  it("keeps a Look readable after a later board update and lets the viewer dismiss it", async () => {
    const board = SCENES[0].script.p1.find((message) => message.kind === "update")
    if (board === undefined) throw new Error("the scene needs a baseline")
    await renderRoutes(
      routes([
        {
          ...board,
          notices: [
            {
              id: "n1",
              kind: "look",
              source: null,
              cards: [{ card: { printing_id: "looked-card", face: 0 }, owner: "p1", position: 0 }],
            },
          ],
        },
        { ...board, update_seq: board.update_seq + 1 },
      ]),
    )
    const user = userEvent.setup()
    await user.click(await screen.findByText("卡片通知（1）"))
    expect(screen.getByText("looked-card")).toBeVisible()
    await user.click(screen.getByRole("button", { name: "關閉通知" }))
    expect(screen.queryByText("looked-card")).toBeNull()
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
