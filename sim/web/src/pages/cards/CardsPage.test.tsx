import { act, screen, waitFor, within } from "@testing-library/react"
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

async function open(path = "/cards") {
  // jsdom reports en-US; the assertions below read the Traditional Chinese copy.
  prefsStore.set({ uiLanguage: "zh-TW" })
  const rendered = await renderRoutes(routes, { initialEntries: [path] })
  // The quick bar appears once the snapshot has loaded, whatever the results are.
  await screen.findByRole("button", { name: "精靈" }, { timeout: 4000 })
  return rendered
}

describe("CardsPage", () => {
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
    const entry = { pages: 3, anchor: "c:bp01-051" }
    await act(() => router.navigate("/cards?class=bishop", { replace: true, state: entry }))
    await userEvent.click(await screen.findByRole("radio", { name: "表格" }))
    expect(router.state.location.search).toBe("?class=bishop&view=table")
    expect(router.state.location.state).toMatchObject(entry)
    await userEvent.click(screen.getByRole("radio", { name: "清單" }))
    expect(router.state.location.state).toMatchObject(entry)
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
    const input = screen.getByRole("combobox")
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
      resultKey: "c:bp01-051",
    })
    // Back lands on the list with the typed text in the URL and the input.
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox")).toHaveValue("bp01-51")
    })
    expect(router.state.location.search).toBe("?q=bp01-51")
  })

  it("marks alias hits, shows all with the see-all link and keeps the text on back", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox")
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
      resultKey: "c:bp01-051",
    })
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox")).toHaveValue("テンプラー")
    })
    expect(router.state.location.state).toMatchObject({ anchor: "c:bp01-051", pages: 1 })
  })

  it("suggests across classes and 'see all' commits the text alone", async () => {
    const { router } = await open("/cards?class=bishop")
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox"), "試作の妖精")
    const list = await screen.findByRole("listbox", {}, { timeout: 4000 })
    expect(within(list).getAllByRole("option").length).toBeGreaterThan(0)
    await user.click(await screen.findByRole("button", { name: /看全部/u }))
    expect(router.state.location.search).toBe("?q=%E8%A9%A6%E4%BD%9C%E3%81%AE%E5%A6%96%E7%B2%BE")
  })

  it("'see all' takes what is typed now, even during the debounce window", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox")
    await user.type(input, "bp01-0")
    await screen.findByRole("listbox")
    await user.type(input, "5")
    await user.click(await screen.findByRole("button", { name: /看全部/u }))
    expect(router.state.location.search).toBe("?q=bp01-05")
  })

  it("ignores clicks on stale suggestions until the list has caught up", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    const input = screen.getByRole("combobox")
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
    await user.type(screen.getByRole("combobox"), " bp01-51 ")
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
    await screen.findByText("張卡片", { exact: false })
    await user.click(screen.getByRole("combobox"))
    expect(await screen.findByText("還沒看過任何卡", { exact: false })).toBeInTheDocument()
    await user.keyboard("{Escape}")
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument()
  })

  it("drops the typed draft when the browser goes back to an older URL", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox"), "foo")
    await user.keyboard("{Enter}")
    expect(router.state.location.search).toBe("?q=foo")
    await router.navigate(-1)
    await waitFor(() => {
      expect(screen.getByRole("combobox")).toHaveValue("")
    })
    expect(router.state.location.search).toBe("")
  })

  it("commits typed text with Enter when nothing matches and the Escape closes the list", async () => {
    const { router } = await open()
    const user = userEvent.setup()
    await user.type(screen.getByRole("combobox"), "nothing-here")
    expect(await screen.findByText("沒有符合的卡")).toBeInTheDocument()
    await user.keyboard("{Enter}")
    expect(router.state.location.search).toBe("?q=nothing-here")
  })
})
