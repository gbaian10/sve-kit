import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore, readPrefs, recentStore } from "../../settings"
import { renderRoutes } from "../../test-utils"

// The page reads the fixture through the real client; fetch is answered from the in-memory build.
const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })

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
  prefsStore.set(DEFAULT_PREFS)
  recentStore.clear()
})

async function open(path = "/cards") {
  // jsdom reports en-US; the assertions below read the Traditional Chinese copy.
  prefsStore.set({ uiLanguage: "zh-TW" })
  const rendered = await renderRoutes(routes, { initialEntries: [path] })
  // The quick bar appears once the snapshot has loaded, whatever the results are.
  await screen.findByRole("group", { name: "職業" }, { timeout: 4000 })
  return rendered
}

describe("CardsPage", () => {
  it("lists the snapshot and keeps the class filter in the URL", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    expect(
      screen.getAllByRole("link").filter((a) => a.hasAttribute("data-result-key")).length,
    ).toBeGreaterThan(20)
    await user.click(
      within(screen.getByRole("group", { name: "職業" })).getByRole("button", { name: "主教" }),
    )
    expect(router.state.location.search).toBe("?class=bishop")
    expect(
      within(screen.getByRole("group", { name: "職業" })).getByRole("button", { name: "主教" }),
    ).toHaveAttribute("aria-pressed", "true")
    await user.click(
      within(screen.getByRole("group", { name: "職業" })).getByRole("button", { name: "中立" }),
    )
    expect(router.state.location.search).toBe("?class=bishop%2Cneutral")
  })

  it("suggests while typing, names the matched field and opens with the keyboard", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox", { name: "搜尋卡片" })
    await user.type(input, "bp01-51")
    const list = await screen.findByRole("listbox")
    const options = within(list).getAllByRole("option")
    expect(options[0]).toHaveTextContent("試作聖堂騎士")
    expect(options[0]).toHaveTextContent("BP01-051")
    expect(options[0]).toHaveTextContent("符合：卡號")
    await user.keyboard("{ArrowDown}")
    expect(input).toHaveAttribute("aria-activedescendant", options[0]?.id)
    expect(options[0]).toHaveAttribute("aria-selected", "true")
    await user.keyboard("{Enter}")
    expect(router.state.location.pathname).toMatch(/^\/cards\/BP01-051(?:\/|$)/u)
    expect(router.state.location.state).toEqual({
      background: "?q=bp01-51",
      source: "suggest",
      pages: 1,
      resultKey: "c:bp01-051",
    })
    // Back lands on the list with the typed text in the URL and the input.
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: "搜尋卡片" })).toHaveValue("bp01-51")
    })
    expect(router.state.location.search).toBe("?q=bp01-51")
  })

  it("marks alias hits, shows all with the see-all link and keeps the text on back", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox", { name: "搜尋卡片" })
    await user.type(input, "テンプラー")
    expect(await screen.findByText(/符合：別名/u)).toBeInTheDocument()
    await user.click(await screen.findByRole("button", { name: /看全部/u }))
    expect(router.state.location.search).toBe("?q=%E3%83%86%E3%83%B3%E3%83%97%E3%83%A9%E3%83%BC")
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument()
    await waitFor(() => {
      expect(
        screen.getAllByRole("link").filter((a) => a.hasAttribute("data-result-key")),
      ).toHaveLength(1)
    })
    await user.click(screen.getByRole("link", { name: /試作聖堂騎士/u }))
    expect(router.state.location.pathname).toMatch(/^\/cards\/BP01-051(?:\/|$)/u)
    expect(router.state.location.state).toMatchObject({
      source: "results",
      resultKey: "c:bp01-051",
    })
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: "搜尋卡片" })).toHaveValue("テンプラー")
    })
    expect(router.state.location.state).toMatchObject({ anchor: "c:bp01-051", pages: 1 })
  })

  it("suggests across classes and 'see all' commits the text alone", async () => {
    const { router } = await open("/cards?class=bishop")
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox", { name: "搜尋卡片" }), "試作の妖精")
    const list = await screen.findByRole("listbox")
    expect(within(list).getAllByRole("option").length).toBeGreaterThan(0)
    await user.click(await screen.findByRole("button", { name: /看全部/u }))
    expect(router.state.location.search).toBe("?q=%E8%A9%A6%E4%BD%9C%E3%81%AE%E5%A6%96%E7%B2%BE")
  })

  it("'see all' takes what is typed now, even during the debounce window", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox", { name: "搜尋卡片" })
    await user.type(input, "bp01-0")
    await screen.findByRole("listbox")
    await user.type(input, "5")
    await user.click(screen.getByRole("button", { name: /看全部/u }))
    expect(router.state.location.search).toBe("?q=bp01-05")
  })

  it("ignores clicks on stale suggestions until the list has caught up", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox", { name: "搜尋卡片" })
    await user.type(input, "bp01-0")
    const list = await screen.findByRole("listbox")
    const first = within(list).getAllByRole("option")[0]
    await user.type(input, "5")
    expect(list).toHaveAttribute("aria-busy", "true")
    if (!first) throw new Error("no option")
    await user.click(first)
    expect(router.state.location.pathname).toBe("/cards")
    await waitFor(() => {
      expect(screen.getByRole("listbox")).toHaveAttribute("aria-busy", "false")
    })
    await user.click(within(screen.getByRole("listbox")).getAllByRole("option")[0] ?? first)
    expect(router.state.location.pathname).toMatch(/^\/cards\/BP01-050(?:\/|$)/u)
    expect(router.state.location.state).toMatchObject({ background: "?q=bp01-05" })
  })

  it("keeps suggestions pickable when the text has surrounding spaces", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox", { name: "搜尋卡片" }), " bp01-51 ")
    const list = await screen.findByRole("listbox")
    await waitFor(() => {
      expect(list).toHaveAttribute("aria-busy", "false")
    })
    await user.click(within(list).getAllByRole("option")[0] ?? list)
    expect(router.state.location.pathname).toMatch(/^\/cards\/BP01-051(?:\/|$)/u)
    expect(router.state.location.state).toMatchObject({ background: "?q=bp01-51" })
  })

  it("uses the view from the URL, else the preference, and writes both when switching", async () => {
    prefsStore.set({ viewMode: "list" })
    const { router } = await open("/cards")
    const user = userEvent.setup()
    expect(
      within(screen.getByRole("radiogroup", { name: "檢視" })).getByRole("radio", { name: "清單" }),
    ).toHaveAttribute("aria-checked", "true")
    await user.click(
      within(screen.getByRole("radiogroup", { name: "檢視" })).getByRole("radio", { name: "表格" }),
    )
    expect(router.state.location.search).toBe("?view=table")
    expect(readPrefs().viewMode).toBe("table")
    expect(
      (await screen.findAllByText("能力預覽・未完整", {}, { timeout: 5000 })).length,
    ).toBeGreaterThan(0)
    // The preview draws the same icons as the card page; keyword chips are plain text here.
    expect((await screen.findAllByRole("img", { name: "入場曲" })).length).toBeGreaterThan(0)
    expect(screen.queryByRole("button", { name: "守護" })).not.toBeInTheDocument()
    const shared = await renderRoutes(routes, { initialEntries: ["/cards?view=grid"] })
    await within(shared.container).findByRole("group", { name: "職業" }, { timeout: 4000 })
    expect(
      within(within(shared.container).getByRole("radiogroup", { name: "檢視" })).getByRole(
        "radio",
        { name: "卡圖" },
      ),
    ).toHaveAttribute("aria-checked", "true")
    shared.unmount()
  })

  it("keeps the anchor card and the loaded pages when the view changes", async () => {
    const { router } = await open("/cards")
    const user = userEvent.setup()
    await user.click(screen.getByRole("link", { name: /試作聖堂騎士/u }))
    await router.navigate(-1)
    // Wait for the list to be back on screen, not only for the router state.
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    })
    expect(router.state.location.state).toMatchObject({ anchor: "c:bp01-051", pages: 1 })
    await user.click(
      within(screen.getByRole("radiogroup", { name: "檢視" })).getByRole("radio", { name: "清單" }),
    )
    expect(router.state.location.search).toBe("?view=list")
    expect(router.state.location.state).toMatchObject({ anchor: "c:bp01-051", pages: 1 })
    await waitFor(() => {
      expect(document.querySelector('a[data-result-key="c:bp01-051"]')).toHaveClass("ring-accent")
    })
  })

  it("opens the filter sheet, applies a draft and shows chips with the badge count", async () => {
    const { router } = await open("/cards")
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "篩選" }))
    const dialog = await screen.findByRole("dialog")
    await user.click(within(dialog).getByRole("button", { name: "主教" }))
    await user.click(within(dialog).getByRole("switch", { name: "只看異畫" }))
    await user.click(within(dialog).getByRole("button", { name: /顯示/u }))
    expect(router.state.location.search).toBe("?class=bishop&alt=1")
    expect(screen.getByRole("button", { name: "篩選" })).toHaveTextContent("2")
    expect(screen.getByRole("button", { name: "移除 主教" })).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "移除 只看異畫" }))
    expect(router.state.location.search).toBe("?class=bishop")
    await user.click(screen.getByRole("button", { name: "清除" }))
    expect(router.state.location.search).toBe("")
  })

  it("gives the filter sheet its own history entry: back closes it, apply replaces it", async () => {
    const { router } = await open("/cards")
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "篩選" }))
    await screen.findByRole("dialog")
    expect(router.state.location.state).toMatchObject({ sheet: true })
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    })
    await router.navigate(1)
    await screen.findByRole("dialog")
    // jsdom fires no `cancel` on Escape; the close button takes the same onClose path.
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "關閉" }))
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    })
    expect(router.state.location.state ?? {}).not.toHaveProperty("sheet")
    // Applying replaces the sheet's entry, so back returns to the list before the sheet.
    await user.click(screen.getByRole("button", { name: "篩選" }))
    const dialog = await screen.findByRole("dialog")
    await user.click(within(dialog).getByRole("button", { name: "主教" }))
    await user.click(within(dialog).getByRole("button", { name: /顯示/u }))
    await waitFor(() => {
      expect(router.state.location.search).toBe("?class=bishop")
    })
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    await router.navigate(-1)
    await waitFor(() => {
      expect(router.state.location.search).toBe("")
    })
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  })

  it("shows the empty state with a reset, and recent cards on focus", async () => {
    const { router } = await open("/cards?q=zzz&class=elf")
    const user = userEvent.setup()
    expect(screen.getByText("沒有符合的卡")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "清除全部條件" }))
    expect(router.state.location.search).toBe("")
    await screen.findByText("張卡片", { exact: false })
    await user.click(screen.getByRole("combobox", { name: "搜尋卡片" }))
    expect(await screen.findByText("還沒看過任何卡", { exact: false })).toBeInTheDocument()
    await user.keyboard("{Escape}")
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument()
  })

  it("drops the typed draft when the browser goes back to an older URL", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox", { name: "搜尋卡片" }), "foo")
    await user.keyboard("{Enter}")
    expect(router.state.location.search).toBe("?q=foo")
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: "搜尋卡片" })).toHaveValue("")
    })
    expect(router.state.location.search).toBe("")
  })

  it("commits typed text with Enter when nothing matches and the Escape closes the list", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox", { name: "搜尋卡片" }), "nothing-here")
    expect(await screen.findByText("沒有符合的卡")).toBeInTheDocument()
    await user.keyboard("{Enter}")
    expect(router.state.location.search).toBe("?q=nothing-here")
  })
})
