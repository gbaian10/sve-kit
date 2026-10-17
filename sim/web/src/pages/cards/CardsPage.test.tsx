import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore, recentStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"
beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))

// The page reads the fixture through the real client; fetch is answered from the in-memory build.
const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })
let failImageMetadata = false
const imageMetadataPaths = new Set(
  (built.manifest["files"] as { path: string; row_counts: unknown }[])
    .filter((file) => JSON.stringify(file.row_counts).includes("printing_image"))
    .map((file) => file.path),
)

beforeAll(() => {
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const path = url.replace(/^.*\/cdn\//u, "")
    if (failImageMetadata && imageMetadataPaths.has(path))
      return Promise.resolve(new Response(null, { status: 503 }))
    const bytes = built.files.get(path)
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
})
afterAll(() => {
  vi.unstubAllGlobals()
})
afterEach(() => {
  failImageMetadata = false
  prefsStore.set(DEFAULT_PREFS)
  recentStore.clear()
})

async function open(path = "/cards", { panel = true } = {}) {
  // jsdom reports en-US; the assertions below read the Traditional Chinese copy.
  prefsStore.set({ uiLanguage: "zh-TW", filterPanelOpen: panel })
  const rendered = await renderRoutes(routes, { initialEntries: [path] })
  // The class row sits outside the panel and appears once the snapshot has loaded, whatever the
  // results are.
  await screen.findByRole("button", { name: "精靈" }, { timeout: 4000 })
  return rendered
}

/** The result count beside the filter button, once the current page has arrived. */
async function expectCount(total: number) {
  await waitFor(() => {
    expect(screen.getByText("個版次")).toHaveTextContent(`${String(total)} 個版次`)
  })
}

describe("CardsPage", () => {
  it("commits a multi-match query with Enter without picking a suggestion", async () => {
    const { router } = await open("/cards?sort=cost")
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox", { name: "搜尋卡片" }), "bp01")
    const list = await screen.findByRole("listbox")
    await waitFor(() => expect(list).toHaveAttribute("aria-busy", "false"))
    await user.keyboard("{Enter}")
    expect(router.state.location.pathname).toBe("/cards")
    expect(router.state.location.search).toBe("?q=bp01&sort=cost")
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument()
  })

  it("opens the explicitly selected suggestion when several cards match", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox", { name: "搜尋卡片" })
    await user.type(input, "bp01")
    const list = await screen.findByRole("listbox")
    await waitFor(() => expect(list).toHaveAttribute("aria-busy", "false"))
    const second = within(list).getAllByRole("option")[1]
    await user.keyboard("{ArrowDown}{ArrowDown}")
    expect(input).toHaveAttribute("aria-activedescendant", second?.id)
    await user.keyboard("{Enter}")
    expect(router.state.location.pathname).toBe("/cards/BP01-002")
  })

  it.each([false, true])(
    "opens the unique result with Enter (before debounce: %s)",
    async (early) => {
      const { router } = await open()
      const user = userEvent.setup()
      await user.type(screen.getByRole("combobox", { name: "搜尋卡片" }), "bp01-051")
      if (!early) {
        const list = await screen.findByRole("listbox")
        await waitFor(() => expect(list).toHaveAttribute("aria-busy", "false"))
      }
      await user.keyboard("{Enter}")
      await waitFor(() => {
        expect(router.state.location.pathname).toBe("/cards/BP01-051")
      })
    },
  )

  it.each(["grid", "table", "list"])(
    "keeps both printings of one card in the %s results with their own images",
    async (view) => {
      await open(`/cards?q=bp01-002&view=${view}`)
      await waitFor(() => {
        const links = document.querySelectorAll("a[data-result-key]")
        expect([...links].map((link) => link.getAttribute("data-result-key"))).toEqual([
          "p:bp01-002",
          "p:bp01-002-alt",
          "p:bp01-020",
          "p:bp01-021",
          "p:bp01-021-sp",
        ])
      })
      expect(screen.getByText("個版次")).toHaveTextContent("5 個版次")
      expect(screen.getByText("共 5 個版次")).toBeVisible()
      if (view === "grid") {
        await waitFor(() => {
          for (const id of ["p:bp01-002", "p:bp01-002-alt"]) {
            expect(
              document.querySelector(`a[data-result-key="${id}"] [data-image-printing]`),
            ).toHaveAttribute("data-image-printing", id)
          }
        })
      }
    },
  )

  it("keeps IME confirmation in the search input", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox", { name: "搜尋卡片" })
    await user.type(input, "bp01")
    const list = await screen.findByRole("listbox")
    await waitFor(() => expect(list).toHaveAttribute("aria-busy", "false"))
    fireEvent.keyDown(input, { key: "Enter", isComposing: true })
    expect(router.state.location.pathname).toBe("/cards")
    expect(router.state.location.search).toBe("")
  })

  it("uses vocabulary rarity chips and shows no PR chip when the vocabulary has none", async () => {
    const { router } = await open()
    const group = screen.getByRole("group", { name: "稀有度" })
    const bronze = within(group).getByRole("button", { name: "銅" })
    await waitFor(() => expect(bronze).toBeEnabled())
    expect(within(group).queryByRole("button", { name: /^PR/u })).not.toBeInTheDocument()
    await userEvent.click(bronze)
    await waitFor(() => expect(bronze).toHaveAttribute("aria-pressed", "true"))
    expect(router.state.location.search).toContain("rarity=bronze")
  })

  it.each([
    ["zh-TW", "銅"],
    ["ja", "ブロンズ"],
    ["en", "Bronze"],
  ] as const)(
    "keeps vocabulary rarity labels when image metadata fails in %s",
    async (language, label) => {
      failImageMetadata = true
      prefsStore.set({ uiLanguage: language })
      await renderRoutes(routes, { initialEntries: ["/cards?view=grid"], language })
      await waitFor(
        () => {
          const cell = document.querySelector("[data-result-key]")
          expect(cell).not.toBeNull()
          expect(cell).toHaveTextContent(label)
        },
        { timeout: 4000 },
      )
      await waitFor(
        () => {
          const slot = document.querySelector("[data-image-printing]")
          expect(slot).toHaveTextContent(
            language === "zh-TW"
              ? "卡圖資訊載入失敗"
              : language === "ja"
                ? "画像情報を読み込めませんでした"
                : "Could not load image metadata",
          )
        },
        { timeout: 4000 },
      )
    },
  )

  it.each(["/sets/UNKNOWN", "/sets/UNKNOWN?set=bp01"])(
    "reports a missing set at %s",
    async (path) => {
      prefsStore.set({ uiLanguage: "zh-TW" })
      await renderRoutes(routes, { initialEntries: [path] })
      expect(await screen.findByText("找不到這個卡包", {}, { timeout: 4000 })).toBeVisible()
      expect(screen.queryByText("沒有符合的卡")).not.toBeInTheDocument()
      expect(document.querySelector("[data-result-key]")).toBeNull()
    },
  )

  it("loads table illustration crops from the independent visible-image channel", async () => {
    await open("/cards?view=table")
    await waitFor(
      () => {
        const thumb = document.querySelector("[data-art-thumb][data-image-printing] img")
        expect(thumb).toHaveAttribute("src", expect.stringContaining("/images/art_"))
        expect(thumb).toHaveAttribute("alt", "")
      },
      { timeout: 4000 },
    )
  })

  it("keeps a set page's own set out of its URL but in the sequence a card walks", async () => {
    const { router } = await open("/sets/BP01")
    const user = userEvent.setup()
    const openFirst = async () => {
      const cell = await waitFor(() => {
        const links = screen.getAllByRole("link").filter((a) => a.hasAttribute("data-result-key"))
        expect(links.length).toBeGreaterThan(0)
        return links[0]
      })
      if (!cell) throw new Error("missing result cell")
      await user.click(cell)
      const { background } = router.state.location.state as { background: string }
      return Object.fromEntries(new URLSearchParams(background))
    }
    expect(await openFirst()).toEqual({ set: "bp01" })
    await act(() => router.navigate(-1))
    await user.click(await screen.findByRole("button", { name: "主教" }))
    expect(router.state.location.pathname).toBe("/sets/BP01")
    expect(router.state.location.search).toBe("?class=bishop")
    expect(await openFirst()).toEqual({ class: "bishop", set: "bp01" })
  })

  it("preserves loaded pages and the anchor when switching views", async () => {
    const { router } = await open("/cards?class=bishop")
    const entry = { pages: 3, anchor: "p:bp01-051" }
    await act(() => router.navigate("/cards?class=bishop", { replace: true, state: entry }))
    await userEvent.click(await screen.findByRole("radio", { name: "表格" }))
    expect(router.state.location.search).toBe("?class=bishop&view=table")
    expect(router.state.location.state).toMatchObject(entry)
    await userEvent.click(screen.getByRole("radio", { name: "清單" }))
    expect(router.state.location.state).toMatchObject(entry)
    const scroll = document.querySelector<HTMLElement>("[data-list-scroll]")
    if (!scroll) throw new Error("missing list scrollport")
    scroll.scrollTop = 240
    scroll.scrollLeft = 120
    fireEvent.scroll(scroll)
    await userEvent.click(screen.getByRole("radio", { name: "表格" }))
    await userEvent.click(screen.getByRole("radio", { name: "清單" }))
    const restored = document.querySelector<HTMLElement>("[data-list-scroll]")
    expect(restored?.scrollTop).toBe(240)
    expect(restored?.scrollLeft).toBe(120)
    expect(router.state.location.state).toMatchObject({
      ...entry,
      listScroll: { top: 240, left: 120 },
    })
  })
  it("resets list scroll on a sort change and restores the previous query on back", async () => {
    const { router } = await open("/cards?view=list")
    const root = document.querySelector<HTMLElement>("[data-list-scroll]")
    if (!root) throw new Error("missing list scrollport")
    root.scrollTop = 240
    root.scrollLeft = 120
    fireEvent.scroll(root)
    await userEvent.click(await screen.findByRole("button", { name: "消費" }))
    await screen.findByRole("columnheader", { name: "消費" })
    await waitFor(() => {
      expect(document.querySelector<HTMLElement>("[data-list-scroll]")?.scrollTop).toBe(0)
    })
    expect(document.querySelector<HTMLElement>("[data-list-scroll]")?.scrollLeft).toBe(0)
    await act(() => router.navigate(-1))
    await waitFor(() => {
      expect(document.querySelector<HTMLElement>("[data-list-scroll]")?.scrollTop).toBe(240)
    })
    expect(document.querySelector<HTMLElement>("[data-list-scroll]")?.scrollLeft).toBe(120)
  })

  it("synchronizes header sorting, direction and the toolbar through the URL", async () => {
    const { router } = await open("/cards?class=bishop&view=list")
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "消費" }))
    expect(await screen.findByRole("combobox", { name: "排序" })).toHaveValue("cost")
    expect(new URLSearchParams(router.state.location.search).get("sort")).toBe("cost")
    await user.click(await screen.findByRole("button", { name: "消費" }))
    expect(new URLSearchParams(router.state.location.search).get("order")).toBe("desc")
    expect(await screen.findByRole("columnheader", { name: "消費" })).toHaveAttribute(
      "aria-sort",
      "descending",
    )
    await user.selectOptions(screen.getByRole("combobox", { name: "排序" }), "atk")
    expect(await screen.findByRole("columnheader", { name: "攻擊力" })).toHaveAttribute(
      "aria-sort",
      "descending",
    )
    expect(new URLSearchParams(router.state.location.search).get("class")).toBe("bishop")
  })

  it("lists the snapshot and keeps the class filter in the URL", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    expect(
      screen.getAllByRole("link").filter((a) => a.hasAttribute("data-result-key")).length,
    ).toBeGreaterThan(20)
    await user.click(screen.getByRole("button", { name: "主教" }))
    expect(router.state.location.search).toBe("?class=bishop")
    expect(screen.getByRole("button", { name: "主教" })).toHaveAttribute("aria-pressed", "true")
    await user.click(screen.getByRole("button", { name: "中立" }))
    expect(router.state.location.search).toBe("?class=bishop%2Cneutral")
  })

  it("suggests while typing, names the matched field and opens with the keyboard", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox", { name: "搜尋卡片" })
    await user.type(input, "bp01-51")
    const list = await screen.findByRole("listbox", {}, { timeout: 4000 })
    const options = within(list).getAllByRole("option")
    expect(options[0]).toHaveTextContent("試作聖堂騎士")
    expect(options[0]).toHaveTextContent("BP01-051")
    expect(options[0]).toHaveTextContent("符合：卡號")
    await waitFor(() => {
      expect(options[0]?.querySelector("img")).toHaveAttribute(
        "src",
        expect.stringContaining("/images/card_"),
      )
      expect(options[0]).toBeInTheDocument()
    })
    await user.keyboard("{ArrowDown}")
    expect(input).toHaveAttribute("aria-activedescendant", options[0]?.id)
    expect(options[0]).toHaveAttribute("aria-selected", "true")
    await user.keyboard("{Enter}")
    expect(router.state.location.pathname).toBe("/cards/BP01-051")
    expect(router.state.location.state).toMatchObject({
      background: "?q=bp01-51",
      source: "suggest",
      pages: 1,
      resultKey: "p:bp01-051",
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
    expect(router.state.location.pathname).toBe("/cards/BP01-051")
    expect(router.state.location.state).toMatchObject({
      source: "results",
      resultKey: "p:bp01-051",
    })
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: "搜尋卡片" })).toHaveValue("テンプラー")
    })
    expect(router.state.location.state).toMatchObject({ anchor: "p:bp01-051", pages: 1 })
  })

  it("suggests across classes and 'see all' commits the text alone", async () => {
    const { router } = await open("/cards?class=bishop")
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox", { name: "搜尋卡片" }), "試作の妖精")
    const list = await screen.findByRole("listbox", {}, { timeout: 4000 })
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
    await user.click(await screen.findByRole("button", { name: /看全部/u }))
    expect(router.state.location.search).toBe("?q=bp01-05")
  })

  it("ignores clicks on stale suggestions until the list has caught up", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox", { name: "搜尋卡片" })
    await user.type(input, "bp01-0")
    const list = await screen.findByRole("listbox", {}, { timeout: 4000 })
    const first = within(list).getAllByRole("option")[0]
    await user.type(input, "5")
    expect(list).toHaveAttribute("aria-busy", "true")
    if (!first) throw new Error("no option")
    await user.click(first)
    expect(router.state.location.pathname).toBe("/cards")
    await waitFor(() => {
      expect(screen.getByRole("listbox")).toHaveAttribute("aria-busy", "false")
    })
    await user.click(within(screen.getByRole("listbox")).getByRole("option", { name: /BP01-050/u }))
    expect(router.state.location.pathname).toBe("/cards/BP01-050")
    expect(router.state.location.state).toMatchObject({ background: "?q=bp01-05" })
  })

  it("keeps suggestions pickable when the text has surrounding spaces", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox", { name: "搜尋卡片" }), " bp01-51 ")
    const list = await screen.findByRole("listbox", {}, { timeout: 4000 })
    await waitFor(() => {
      expect(list).toHaveAttribute("aria-busy", "false")
    })
    await user.click(within(list).getAllByRole("option")[0] ?? list)
    expect(router.state.location.pathname).toBe("/cards/BP01-051")
    expect(router.state.location.state).toMatchObject({ background: "?q=bp01-51" })
  })

  it("shows the empty state with a reset, and recent cards on focus", async () => {
    const { router } = await open("/cards?q=zzz&class=elf")
    const user = userEvent.setup()
    expect(screen.getByText("沒有符合的卡")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "清除全部條件" }))
    expect(router.state.location.search).toBe("")
    await screen.findByText("個版次")
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
    await waitFor(() => {
      expect(router.state.location.search).toBe("?q=foo")
    })
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

  describe("filter panel", () => {
    it("expands from the search bar button, takes no space while collapsed and remembers the choice", async () => {
      await open("/cards?class=bishop", { panel: false })
      const button = screen.getByRole("button", { name: "篩選" })
      expect(button).toHaveAttribute("aria-expanded", "false")
      // Collapsed: the condition chips keep the only clear entry on screen.
      const conditions = screen.getByRole("group", { name: "目前條件" })
      expect(within(conditions).getByRole("button", { name: "清除" })).toBeVisible()
      const panel = document.getElementById(button.getAttribute("aria-controls") ?? "")
      expect(panel).not.toBeNull()
      expect(panel).not.toBeVisible()
      // The class row stays outside the panel, visible while collapsed, and the badge leaves it out.
      const classRow = screen.getByRole("group", { name: "職業" })
      expect(classRow).toBeVisible()
      expect(panel).not.toContainElement(classRow)
      expect(within(classRow).getByRole("button", { name: "主教" })).toHaveAttribute(
        "aria-pressed",
        "true",
      )
      expect(button).toHaveTextContent("")
      await userEvent.click(button)
      expect(button).toHaveAttribute("aria-expanded", "true")
      expect(panel).toBeVisible()
      expect(panel).not.toContainElement(classRow)
      expect(prefsStore.get().filterPanelOpen).toBe(true)
      // The chip row stays the one clear entry; the panel has none of its own.
      expect(within(conditions).getByRole("button", { name: "清除" })).toBeVisible()
      expect(
        within(screen.getByRole("region", { name: "篩選條件" })).queryByRole("button", {
          name: /^清除/u,
        }),
      ).not.toBeInTheDocument()
      await userEvent.click(button)
      expect(button).toHaveAttribute("aria-expanded", "false")
      expect(panel).not.toBeVisible()
      expect(prefsStore.get().filterPanelOpen).toBe(false)
    })

    it("unions the values of one stat, intersects stats and writes both to the URL", async () => {
      const { router } = await open()
      const user = userEvent.setup()
      const cost = screen.getByRole("group", { name: "消費" })
      const attack = screen.getByRole("group", { name: "攻擊力" })
      const one = within(cost).getByRole("button", { name: "1" })
      const two = within(cost).getByRole("button", { name: "2" })
      await user.click(one)
      expect(router.state.location.search).toBe("?cost=1")
      expect(one).toHaveAttribute("aria-pressed", "true")
      await expectCount(5)
      await user.click(two)
      expect(router.state.location.search).toBe("?cost=1%2C2")
      await expectCount(12)
      await user.click(one)
      expect(router.state.location.search).toBe("?cost=2")
      expect(one).toHaveAttribute("aria-pressed", "false")
      await expectCount(7)
      await user.click(within(attack).getByRole("button", { name: "1" }))
      expect(router.state.location.search).toBe("?cost=2&atk=1")
      await expectCount(1)
      await user.click(within(attack).getByRole("button", { name: "1" }))
      await user.click(within(attack).getByRole("button", { name: "0" }))
      expect(router.state.location.search).toBe("?cost=2&atk=0")
      expect(await screen.findByText("沒有符合的卡")).toBeVisible()
    })

    it("restores every button from the URL and clears all conditions at once", async () => {
      const { router } = await open("/cards?cost=2,7&atk=1&def=10&class=bishop")
      const cost = screen.getByRole("group", { name: "消費" })
      const defense = screen.getByRole("group", { name: "生命值" })
      expect(within(cost).getByRole("button", { name: "2" })).toHaveAttribute(
        "aria-pressed",
        "true",
      )
      expect(within(cost).getByRole("button", { name: "7+" })).toHaveAttribute(
        "aria-pressed",
        "true",
      )
      expect(within(cost).getByRole("button", { name: "3" })).toHaveAttribute(
        "aria-pressed",
        "false",
      )
      expect(
        within(screen.getByRole("group", { name: "攻擊力" })).getByRole("button", { name: "1" }),
      ).toHaveAttribute("aria-pressed", "true")
      expect(within(defense).getByRole("button", { name: "10+" })).toHaveAttribute(
        "aria-pressed",
        "true",
      )
      expect(screen.getByRole("button", { name: "主教" })).toHaveAttribute("aria-pressed", "true")
      const conditions = screen.getByRole("group", { name: "目前條件" })
      await userEvent.click(within(conditions).getByRole("button", { name: "清除" }))
      expect(router.state.location.search).toBe("")
      expect(within(cost).getByRole("button", { name: "2" })).toHaveAttribute(
        "aria-pressed",
        "false",
      )
      expect(screen.queryByRole("group", { name: "目前條件" })).not.toBeInTheDocument()
    })

    it("lists every panel condition as a removable chip in panel order", async () => {
      const { router } = await open(
        "/cards?class=bishop&cost=1,7&atk=10&rarity=bronze,pr,pr:gr&type=follower,token,leader:gold,token:spell",
      )
      const conditions = screen.getByRole("group", { name: "目前條件" })
      const chips = () =>
        within(conditions)
          .getAllByRole("button", { name: /^移除條件/u })
          .map((button) => button.getAttribute("aria-label")?.replace(/^移除條件[：:]?\s*/u, ""))
      await waitFor(() => {
        expect(chips()).toEqual([
          "主教",
          "消費 1",
          "消費 7+",
          "攻擊力 10+",
          "PR",
          "PR（GR）",
          "銅",
          "從者",
          "Token",
          "Token・法術",
          "主戰者・金框",
        ])
      })
      // The whole-PR chip is the grey badge; the appearance chip its own badge; the fixture's "銅"
      // is not a ladder code (no badge, sorted after PR like in the panel) and stays text.
      const prChip = within(conditions).getByRole("button", { name: "移除條件：PR" }).parentElement
      expect(prChip?.querySelector("img")).toHaveAttribute("alt", "PR")
      const search = () => new URLSearchParams(router.state.location.search)
      await userEvent.click(within(conditions).getByRole("button", { name: "移除條件：消費 7+" }))
      expect(search().get("cost")).toBe("1")
      expect(search().get("class")).toBe("bishop")
      expect(search().get("atk")).toBe("10")
      await userEvent.click(within(conditions).getByRole("button", { name: "移除條件：PR（GR）" }))
      expect(search().get("rarity")).toBe("bronze,pr")
      await userEvent.click(
        within(conditions).getByRole("button", { name: "移除條件：Token・法術" }),
      )
      expect(chips()).not.toContain("Token・法術")
      expect(chips()).toContain("Token")
      // The set dropdown's choice gets a chip with the dropdown's own wording.
      await userEvent.click(screen.getByRole("button", { name: /卡包/u }))
      const first = screen.getAllByRole("checkbox", { name: /^BP01 / })[0]
      if (!first) throw new Error("missing set")
      await userEvent.click(first)
      expect(chips().at(-1)).toBe(first.getAttribute("aria-label"))
    })

    it("shows a clear button only beside a section with a selection and clears that section alone", async () => {
      const { router } = await open("/cards?cost=2,7&rarity=bronze")
      const cost = screen.getByRole("group", { name: "消費" })
      const rarity = screen.getByRole("group", { name: "稀有度" })
      expect(within(cost).getByRole("button", { name: "清除消費" })).toBeVisible()
      expect(within(rarity).getByRole("button", { name: "清除稀有度" })).toBeVisible()
      expect(
        within(screen.getByRole("group", { name: "攻擊力" })).queryByRole("button", {
          name: /^清除/u,
        }),
      ).not.toBeInTheDocument()
      expect(
        within(screen.getByRole("group", { name: "卡面版本" })).queryByRole("button", {
          name: /^清除/u,
        }),
      ).not.toBeInTheDocument()
      await userEvent.click(within(cost).getByRole("button", { name: "清除消費" }))
      expect(router.state.location.search).toBe("?rarity=bronze")
      expect(within(cost).queryByRole("button", { name: "清除消費" })).not.toBeInTheDocument()
      expect(within(cost).getByRole("button", { name: "2" })).toHaveAttribute(
        "aria-pressed",
        "false",
      )
      expect(within(rarity).getByRole("button", { name: "銅" })).toHaveAttribute(
        "aria-pressed",
        "true",
      )
      await userEvent.click(within(rarity).getByRole("button", { name: "清除稀有度" }))
      expect(router.state.location.search).toBe("")
      expect(within(rarity).queryByRole("button", { name: "清除稀有度" })).not.toBeInTheDocument()
    })

    it("switches the row unit and the card edition from the panel", async () => {
      const { router } = await open()
      const user = userEvent.setup()
      const rows = screen.getByRole("group", { name: "每列單位" })
      expect(within(rows).getByRole("button", { name: "每個版次一行" })).toHaveAttribute(
        "aria-pressed",
        "true",
      )
      await user.click(within(rows).getByRole("button", { name: "每張卡一行" }))
      expect(router.state.location.search).toBe("?unit=card")
      await waitFor(() => {
        expect(screen.getByText("張卡片")).toBeInTheDocument()
      })
      const edition = screen.getByRole("group", { name: "卡面版本" })
      expect(within(edition).getByRole("button", { name: "日版" })).toHaveAttribute(
        "aria-pressed",
        "true",
      )
      await user.click(within(edition).getByRole("button", { name: "英版" }))
      expect(router.state.location.search).toBe("?unit=card&ed=en")
      expect(prefsStore.get().cardEdition).toBe("en")
    })
  })
})
