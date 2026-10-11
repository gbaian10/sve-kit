import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore, recentStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))

beforeAll(() => {
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const bytes = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
})
afterAll(() => {
  vi.unstubAllGlobals()
})
afterEach(() => {
  vi.restoreAllMocks()
  prefsStore.set(DEFAULT_PREFS)
  recentStore.clear()
})

async function open(path = "/") {
  prefsStore.set({ uiLanguage: "zh-TW" })
  const rendered = await renderRoutes(routes, { initialEntries: [path] })
  // The random shortcut enables once the basic catalog is complete.
  await waitFor(
    () => {
      expect(screen.getByRole("button", { name: "隨機一張" })).toBeEnabled()
    },
    { timeout: 4000 },
  )
  return rendered
}

const main = () => within(screen.getByRole("main"))

async function goHome(router: Awaited<ReturnType<typeof open>>["router"]) {
  await router.navigate("/")
  await screen.findByRole("region", { name: "官方最新消息" })
}

describe("home search", () => {
  it("opens the first suggestion with Enter and keeps the text when coming back", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox"), "bp01-51")
    const list = await screen.findByRole("listbox")
    expect(within(list).getAllByRole("option")[0]).toHaveTextContent("BP01-051")
    await user.keyboard("{Enter}")
    expect(router.state.location.pathname).toBe("/cards/BP01-051")
    expect(router.state.location.state).toEqual({
      background: "?q=bp01-51",
      source: "suggest",
      pages: 1,
      resultKey: "c:bp01-051",
    })
    expect(recentStore.get()[0]).toBe("p:bp01-051")
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox")).toHaveValue("bp01-51")
    })
  })

  it("opens the highlighted suggestion", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox")
    await user.type(input, "試作")
    const options = within(await screen.findByRole("listbox")).getAllByRole("option")
    expect(options.length).toBeGreaterThan(1)
    await user.keyboard("{ArrowDown}{ArrowDown}")
    expect(input).toHaveAttribute("aria-activedescendant", options[1]?.id)
    const second = options[1]?.textContent ?? ""
    await user.keyboard("{Enter}")
    const cardNo = decodeURIComponent(router.state.location.pathname.replace("/cards/", ""))
    expect(second).toContain(cardNo)
  })

  // Positive state awaiting data: real snapshots have no search_alias rows yet.
  it("marks alias hits in the home suggestions", async () => {
    await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox"), "テンプラー")
    const list = await screen.findByRole("listbox", { name: "建議" })
    expect(within(list).getAllByRole("option")[0]).toHaveTextContent("符合：別名")
  })

  it("waits for the suggestions when Enter comes inside the debounce window", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox"), "bp01-51{Enter}")
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/cards/BP01-051")
    })
  })

  it("keeps IME confirmation keys inside the search input", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox")
    await user.type(input, "bp01-51")
    await screen.findByRole("listbox")
    fireEvent.keyDown(input, { key: "ArrowDown", isComposing: true })
    expect(input).not.toHaveAttribute("aria-activedescendant")
    fireEvent.keyDown(input, { key: "Enter", isComposing: true })
    expect(router.state.location.pathname).toBe("/")
    await user.keyboard("{Enter}")
    expect(router.state.location.pathname).toBe("/cards/BP01-051")
  })

  it("cancels an Enter waiting for debounce when the suggestions are dismissed", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox")
    await user.type(input, "bp01-51{Enter}{Escape}")
    await user.tab()
    await user.click(input)
    await screen.findByRole("listbox")
    expect(router.state.location.pathname).toBe("/")
  })

  it("keeps the text when returning from a recent card or a shortcut", async () => {
    recentStore.push("p:bp01-001")
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox"), "bp01-51")
    const recent = within(screen.getByRole("region", { name: "最近查看" }))
    await user.click(recent.getByRole("link"))
    expect(router.state.location.pathname).toBe("/cards/BP01-001")
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox")).toHaveValue("bp01-51")
    })
    await user.click(screen.getByRole("combobox"))
    await user.click(main().getByRole("link", { name: "進階搜尋" }))
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox")).toHaveValue("bp01-51")
    })
  })

  it("takes the last input to the list with 'see all' and shows the empty list on no match", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox")
    await user.type(input, "試作")
    await user.click(await screen.findByRole("button", { name: /看全部/u }))
    expect(router.state.location.pathname).toBe("/cards")
    expect(router.state.location.search).toBe("?q=%E8%A9%A6%E4%BD%9C")
    await goHome(router)
    await user.type(screen.getByRole("combobox"), "zzqqxx")
    expect(await screen.findByText("沒有符合的卡")).toBeVisible()
    expect(screen.queryByRole("button", { name: /看全部/u })).toBeNull()
    await user.keyboard("{Enter}")
    expect(router.state.location.pathname).toBe("/cards")
    expect(router.state.location.search).toBe("?q=zzqqxx")
  })

  it("reaches the planned routes from the shortcuts and opens a random card", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.click(main().getByRole("link", { name: "進階搜尋" }))
    expect(router.state.location.pathname).toBe("/cards")
    await goHome(router)
    await user.click(main().getByRole("link", { name: "依卡包" }))
    expect(router.state.location.pathname).toBe("/sets")
    await goHome(router)
    const random = screen.getByRole("button", { name: "隨機一張" })
    await waitFor(() => {
      expect(random).toBeEnabled()
    })
    await user.click(random)
    await waitFor(() => {
      expect(router.state.location.pathname).toMatch(/^\/cards\/[^/]+$/u)
    })
    // A random card of the Japanese edition, opened on its Japanese printing.
    expect(recentStore.get()).toHaveLength(1)
    expect(recentStore.get()[0]).not.toMatch(/-en$/u)
  })

  it("disables the old random pick while changing edition and opens the new edition", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const random = screen.getByRole("button", { name: "隨機一張" })
    act(() => {
      prefsStore.set({ cardEdition: "en" })
    })
    expect(random).toBeDisabled()
    await waitFor(
      () => {
        expect(random).toBeEnabled()
      },
      { timeout: 4000 },
    )
    await user.click(random)
    expect(router.state.location.pathname).toMatch(/^\/cards\/(?:BP01|SD01)EN-/u)
    expect(recentStore.get()[0]).toMatch(/-en$/u)
  })
})

describe("recently viewed", () => {
  it("shows known cards once, skips unknown ids without deleting them, and clears", async () => {
    recentStore.push("p:unknown")
    recentStore.push("p:bp01-051")
    recentStore.push("p:bp01-051")
    await open()
    const section = within(screen.getByRole("region", { name: "最近查看" }))
    const links = await section.findAllByRole("link")
    expect(links).toHaveLength(1)
    expect(links[0]).toHaveTextContent("BP01-051")
    expect(links[0]).toHaveAttribute("href", "/cards/BP01-051")
    expect(recentStore.get()).toEqual(["p:bp01-051", "p:unknown"])
    await userEvent.click(section.getByRole("button", { name: "清除" }))
    expect(recentStore.get()).toEqual([])
    expect(section.getByText(/還沒看過任何卡/u)).toBeVisible()
  })

  it("can clear history containing only unknown printing ids", async () => {
    recentStore.push("p:unknown")
    await open()
    const recent = within(screen.getByRole("region", { name: "最近查看" }))
    expect(recent.queryByRole("link")).toBeNull()
    expect(recentStore.get()).toEqual(["p:unknown"])
    await userEvent.click(recent.getByRole("button", { name: "清除" }))
    expect(recentStore.get()).toEqual([])
  })

  it("keeps working for the visit when storage refuses writes", async () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked")
    })
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox"), "bp01-51")
    await screen.findByRole("listbox")
    await user.keyboard("{Enter}")
    expect(router.state.location.pathname).toBe("/cards/BP01-051")
    await router.navigate(-1)
    const section = within(await screen.findByRole("region", { name: "最近查看" }))
    expect(await section.findByRole("link")).toHaveTextContent("BP01-051")
  })
})

describe("sets, new pack and official news", () => {
  it("lists dated booster sets of the selected edition with real dates and no percentages", async () => {
    await open()
    const sets = within(screen.getByRole("region", { name: "依卡包・最近三彈" }))
    expect(sets.getByRole("link", { name: "全部卡包" })).toHaveAttribute("href", "/sets")
    const entry = sets.getByRole("link", { name: /BP01/u })
    expect(entry).toHaveAttribute("href", "/sets/BP01")
    expect(entry).toHaveTextContent("2026-02-01 發售")
    // The one dated booster set leaves room for two more, so the undated rule is spelled out.
    expect(sets.getByText(/發售日未知的不列入/u)).toBeVisible()
    const banner = within(screen.getByRole("region", { name: "試作補充包第1彈" }))
    expect(banner.getByText(/新卡包/u)).toHaveTextContent("新卡包・2026-02-01 發售")
    expect(banner.getByRole("link", { name: "看卡表" })).toHaveAttribute("href", "/sets/BP01")
    // The fixture's booster holds 21 Japanese cards; 20 of them have a confident zh-Hant name.
    expect(await banner.findByText(/已收錄/u)).toHaveTextContent("已收錄 21 種")
    expect(banner.getByText(/繁中譯名/u)).toHaveTextContent("有繁中譯名 20")
    expect(banner.queryByText(/待校對/u)).toBeNull()
    expect(main().queryByText(/%|／/u)).toBeNull()
    prefsStore.set({ cardEdition: "en" })
    expect(await sets.findByText("2026-04-01 發售")).toBeVisible()
  })

  it("only links to official news, following the card edition", async () => {
    await open()
    const news = within(screen.getByRole("region", { name: "官方最新消息" }))
    const [site, x] = news.getAllByRole("link")
    expect(site).toHaveAttribute("href", "https://shadowverse-evolve.com/news/")
    expect(x).toHaveAttribute("href", "https://x.com/shadowverse_ev")
    for (const link of [site, x]) {
      expect(link).toHaveAttribute("target", "_blank")
      expect(link).toHaveAttribute("rel", "noopener noreferrer")
    }
    prefsStore.set({ cardEdition: "en" })
    await waitFor(() => {
      expect(news.getAllByRole("link")[0]).toHaveAttribute(
        "href",
        "https://en.shadowverse-evolve.com/news/",
      )
    })
    expect(news.getAllByRole("link")[1]).toHaveAttribute("href", "https://x.com/svevolve_en")
    expect(main().queryByText(/登入|同步|繼續對戰/u)).toBeNull()
  })
})
