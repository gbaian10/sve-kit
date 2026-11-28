import "fake-indexeddb/auto"

import { act, fireEvent, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

import { routes } from "../../app/routes"
import { encodeDeckCode } from "../../domain/deck/code"
import { createDeck } from "../../domain/deck/model"
import type { Deck } from "../../domain/deck/types"
import { decksStore, DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"
import { routeFixtureFetch, stubFixtureFetch } from "../../test-utils/fixture-fetch"

beforeEach(() => vi.stubGlobal("Worker", TestAppWorker))
beforeAll(async () => {
  await stubFixtureFetch()
})
afterAll(() => {
  vi.unstubAllGlobals()
})
afterEach(async () => {
  prefsStore.set(DEFAULT_PREFS)
  await decksStore.reload()
  const state = decksStore.get()
  if (state.status === "ready") for (const deck of state.decks) await decksStore.remove(deck.id)
})

function storedDecks(): readonly Deck[] {
  const state = decksStore.get()
  return state.status === "ready" ? state.decks : []
}

async function openImport(path = "/decks/import") {
  prefsStore.set({ uiLanguage: "zh-TW" })
  return renderRoutes(routes, { initialEntries: ["/decks", path] })
}

describe("DeckImportPage", () => {
  it("previews a pasted list, lists what it cannot use, and creates the deck", async () => {
    const { router } = await openImport()
    fireEvent.change(screen.getByRole("textbox", { name: "牌組文字、分享碼或 DeckLog 代碼" }), {
      target: { value: "# 王族\n3 BP01-001\n1 SD01-L01\n2 BP99-999\nhello\n## 候補\n1 BP01-004" },
    })
    const preview = await screen.findByRole("region", { name: "辨識結果" }, { timeout: 25_000 })
    expect(within(preview).getByText("主牌組 3 張・進化牌組 0 張・候補 1 張")).toBeInTheDocument()
    expect(within(preview).getByText("BP99-999 ×2")).toBeInTheDocument()
    expect(within(preview).getByText("5：hello")).toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole("button", { name: "建立牌組" }))
    const [deck] = storedDecks()
    expect(deck).toMatchObject({
      name: "王族",
      edition: "jp",
      basis: { kind: "class", classCode: "royal" },
      leader: { cardId: "c:sd01-001" },
      main: [{ cardId: "c:bp01-001", count: 3 }],
      side: [{ cardId: "c:sd01-002", count: 1 }],
    })
    await waitFor(() => {
      expect(router.state.location.pathname).toBe(`/decks/${deck?.id ?? ""}`)
    })
    // The page replaced itself, so back returns to the list.
    await act(async () => {
      await router.navigate(-1)
    })
    expect(router.state.location.pathname).toBe("/decks")
  }, 60_000)

  it("draws the deck from the small card images before the deck is made", async () => {
    // jsdom has no canvas or image decoding; stand-ins record which pictures are drawn.
    const drawn: string[] = []
    const context = new Proxy<Record<string, unknown>>(
      { measureText: (text: string) => ({ width: text.length * 10 }) },
      {
        get: (target, key) =>
          typeof key === "string" && key in target
            ? target[key]
            : (...args: unknown[]) => {
                if (key === "drawImage" && args[0] instanceof HTMLImageElement)
                  drawn.push(args[0].src)
              },
      },
    )
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
      context as unknown as CanvasRenderingContext2D,
    )
    const decode = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, "decode")
    Object.defineProperty(HTMLImageElement.prototype, "decode", {
      configurable: true,
      value: () => Promise.resolve(),
    })
    try {
      await openImport()
      fireEvent.change(screen.getByRole("textbox", { name: "牌組文字、分享碼或 DeckLog 代碼" }), {
        target: { value: "# 王族\n3 BP01-001\n1 SD01-L01" },
      })
      const preview = await screen.findByRole("region", { name: "辨識結果" }, { timeout: 25_000 })
      await within(preview).findByRole("img", { name: "王族 的牌組圖片" }, { timeout: 25_000 })
      // The leader and the one main card, from the small files (redraws may repeat them).
      expect(new Set(drawn).size).toBe(2)
      expect(drawn.every((src) => src.includes("/card_s/"))).toBe(true)
    } finally {
      if (decode) Object.defineProperty(HTMLImageElement.prototype, "decode", decode)
      else Reflect.deleteProperty(HTMLImageElement.prototype, "decode")
      vi.restoreAllMocks()
    }
  }, 60_000)

  it("opens a share link with its preview and lists ids the snapshot does not have", async () => {
    const code = encodeDeckCode({
      edition: "jp",
      format: "crossover",
      basis: { kind: "class", classCode: "royal" },
      leader: 2001,
      main: [
        { intId: 1001, count: 3 },
        { intId: 1999, count: 1 },
      ],
      evolve: [],
      side: [],
      name: "分享的牌組",
    })
    await openImport(`/decks/import?d=${code}`)
    const preview = await screen.findByRole("region", { name: "辨識結果" }, { timeout: 25_000 })
    expect(within(preview).getByText("名稱：分享的牌組")).toBeInTheDocument()
    expect(within(preview).getByText("#1999 ×1")).toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole("button", { name: "建立牌組" }))
    expect(storedDecks()[0]).toMatchObject({
      name: "分享的牌組",
      format: "crossover",
      leader: { cardId: "c:sd01-001" },
      main: [{ cardId: "c:bp01-001", count: 3 }],
    })
  }, 60_000)

  it("says a damaged code cannot be read and creates nothing", async () => {
    await openImport("/decks/import?d=SVEK-AQ")
    expect(await screen.findByRole("alert")).toHaveTextContent("無法讀取這個分享碼。")
    expect(screen.getByRole("button", { name: "建立牌組" })).toBeDisabled()
  })

  it("exports a deck as text and as a share code that imports back to the same deck", async () => {
    const deck: Deck = {
      ...createDeck(
        { name: "往返", edition: "jp", basis: { kind: "class", classCode: "royal" } },
        new Date("2026-11-03T00:00:00.000Z"),
      ),
      leader: { cardId: "c:sd01-001" },
      main: [
        { cardId: "c:bp01-001", count: 3 },
        { cardId: "c:bp01-002", count: 2 },
      ],
      side: [{ cardId: "c:bp01-003", count: 1 }],
    }
    await decksStore.save(deck)
    prefsStore.set({ uiLanguage: "zh-TW" })
    const { router } = await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
    const user = userEvent.setup()
    // Below 1180 px the deck, with its export button, is a sheet over the pool.
    await user.click(await screen.findByRole("button", { name: "打開牌組" }, { timeout: 25_000 }))
    await user.click(await screen.findByRole("button", { name: "匯出／分享" }, { timeout: 25_000 }))
    const dialog = await screen.findByRole("dialog", { name: "匯出／分享" })
    const text = await within(dialog).findByRole<HTMLTextAreaElement>("textbox", { name: "文字" })
    expect(text.value).toContain("# 往返")
    expect(text.value).toContain("3 BP01-001")
    expect(text.value).toContain("## 候補區 (1)")
    await user.click(within(dialog).getByRole("radio", { name: "分享" }))
    const link = within(dialog).getByRole<HTMLTextAreaElement>("textbox", { name: "分享連結" })
    const url = new URL(link.value)
    await act(async () => {
      await router.navigate(`${url.pathname}${url.search}`)
    })
    const preview = await screen.findByRole("region", { name: "辨識結果" }, { timeout: 25_000 })
    expect(within(preview).getByText("主牌組 5 張・進化牌組 0 張・候補 1 張")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "建立牌組" }))
    const copy = storedDecks().find((item) => item.id !== deck.id)
    expect(copy).toMatchObject({
      name: "往返",
      edition: "jp",
      basis: deck.basis,
      leader: deck.leader,
      main: deck.main,
      side: deck.side,
    })
  }, 60_000)

  it("carries a chosen reprint through the share code and through the text list", async () => {
    // BP01-002a is a reprint known to the main thread only once every printing has loaded.
    const deck: Deck = {
      ...createDeck(
        { name: "SVEK-like name", edition: "jp", basis: { kind: "class", classCode: "royal" } },
        new Date("2026-11-03T00:00:00.000Z"),
      ),
      main: [{ cardId: "c:bp01-002", count: 2, printingId: "p:bp01-002-alt" }],
    }
    await decksStore.save(deck)
    prefsStore.set({ uiLanguage: "zh-TW" })
    const { router } = await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
    const user = userEvent.setup()
    // Below 1180 px the deck, with its export button, is a sheet over the pool.
    await user.click(await screen.findByRole("button", { name: "打開牌組" }, { timeout: 25_000 }))
    await user.click(await screen.findByRole("button", { name: "匯出／分享" }, { timeout: 25_000 }))
    const dialog = await screen.findByRole("dialog", { name: "匯出／分享" })
    const text = await within(dialog).findByRole<HTMLTextAreaElement>(
      "textbox",
      { name: "文字" },
      { timeout: 25_000 },
    )
    expect(text.value).toContain("2 BP01-002a")
    const list = text.value
    await user.click(within(dialog).getByRole("radio", { name: "分享" }))
    const link = within(dialog).getByRole<HTMLTextAreaElement>("textbox", { name: "分享連結" })
    expect(within(dialog).queryByText(/沒有永久編號/u)).not.toBeInTheDocument()
    const url = new URL(link.value)

    for (const route of [`${url.pathname}${url.search}`, "/decks/import"]) {
      await act(async () => {
        await router.navigate(route)
      })
      const input = await screen.findByRole("textbox", { name: "牌組文字、分享碼或 DeckLog 代碼" })
      // The second pass pastes the text list, whose name line looks like a code.
      if (route === "/decks/import") fireEvent.change(input, { target: { value: list } })
      const preview = await screen.findByRole("region", { name: "辨識結果" }, { timeout: 25_000 })
      expect(within(preview).getByText("主牌組 2 張・進化牌組 0 張・候補 0 張")).toBeInTheDocument()
      await user.click(screen.getByRole("button", { name: "建立牌組" }))
      await waitFor(() => {
        expect(router.state.location.pathname).not.toBe("/decks/import")
      })
    }
    const copies = storedDecks().filter((item) => item.id !== deck.id)
    expect(copies).toHaveLength(2)
    for (const copy of copies) {
      expect(copy.name).toBe("SVEK-like name")
      expect(copy.main).toEqual(deck.main)
    }
  }, 60_000)

  it("stays on the page with the pasted list when the deck cannot be saved", async () => {
    await openImport()
    fireEvent.change(screen.getByRole("textbox", { name: "牌組文字、分享碼或 DeckLog 代碼" }), {
      target: { value: "3 BP01-001" },
    })
    await screen.findByRole("region", { name: "辨識結果" }, { timeout: 25_000 })
    const save = vi.spyOn(decksStore, "save").mockRejectedValueOnce(new Error("quota"))
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "建立牌組" }))
    expect(await screen.findByText("牌組沒有存成功，請再試一次。")).toBeInTheDocument()
    expect(screen.getByRole("textbox", { name: "牌組文字、分享碼或 DeckLog 代碼" })).toHaveValue(
      "3 BP01-001",
    )
    save.mockRestore()
    await user.click(screen.getByRole("button", { name: "建立牌組" }))
    await waitFor(() => {
      expect(storedDecks().map((deck) => deck.main)).toEqual([[{ cardId: "c:bp01-001", count: 3 }]])
    })
  }, 60_000)

  it("says so when the browser cannot draw the deck image", async () => {
    const deck = createDeck(
      { name: "圖片", edition: "jp", basis: { kind: "class", classCode: "royal" } },
      new Date("2026-11-03T00:00:00.000Z"),
    )
    await decksStore.save(deck)
    prefsStore.set({ uiLanguage: "zh-TW" })
    await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
    const user = userEvent.setup()
    // Below 1180 px the deck, with its export button, is a sheet over the pool.
    await user.click(await screen.findByRole("button", { name: "打開牌組" }, { timeout: 25_000 }))
    await user.click(await screen.findByRole("button", { name: "匯出／分享" }, { timeout: 25_000 }))
    const dialog = await screen.findByRole("dialog", { name: "匯出／分享" })
    await user.click(within(dialog).getByRole("radio", { name: "圖片" }))
    expect(within(dialog).getByRole("radio", { name: "高清（看得清卡文）" })).toBeInTheDocument()
    // jsdom has no canvas, as some locked-down browsers do not; the dialog says so instead of hanging.
    await user.click(within(dialog).getByRole("button", { name: "產生圖片" }))
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("圖片產生失敗")
  }, 60_000)

  it("reads a DeckLog link through the Worker and lists the card numbers it cannot match", async () => {
    const asked: string[] = []
    routeFixtureFetch((url) => {
      if (!url.startsWith("/api/decklog/")) return undefined
      asked.push(url)
      return Promise.resolve(
        Response.json({
          region: "jp",
          code: "AB12C",
          format: "N",
          className: "ロイヤル",
          leader: [{ cardNo: "SD01-L01", count: 1 }],
          main: [
            { cardNo: "BP01-001", count: 3 },
            { cardNo: "BP01-004", count: 2 },
          ],
          evolve: [{ cardNo: "BP99-999", count: 2 }],
        }),
      )
    })
    try {
      await openImport()
      const user = userEvent.setup()
      fireEvent.change(screen.getByRole("textbox", { name: "牌組文字、分享碼或 DeckLog 代碼" }), {
        target: { value: "https://decklog.bushiroad.com/view/ab12c" },
      })
      expect(await screen.findByText("日版 DeckLog 的牌組 AB12C。")).toBeInTheDocument()
      // A link names its edition, so there is no edition to pick.
      expect(screen.queryByRole("radiogroup", { name: "卡片版本" })).not.toBeInTheDocument()
      await user.click(screen.getByRole("button", { name: "從 DeckLog 讀取" }))
      const preview = await screen.findByRole("region", { name: "辨識結果" }, { timeout: 25_000 })
      expect(asked).toEqual(["/api/decklog/jp/AB12C"])
      expect(within(preview).getByText("來源：DeckLog AB12C")).toBeInTheDocument()
      // DeckLog's own wording is marked as Japanese inside the Chinese sentence.
      expect(within(preview).getByText("ロイヤル")).toHaveAttribute("lang", "ja")
      expect(within(preview).getByText("主牌組 5 張・進化牌組 0 張・候補 0 張")).toBeInTheDocument()
      expect(within(preview).getByText("BP99-999 ×2")).toBeInTheDocument()
      expect(screen.getByRole("button", { name: "從 DeckLog 讀取" })).toBeDisabled()
      await user.click(screen.getByRole("button", { name: "建立牌組" }))
      await waitFor(() => {
        expect(storedDecks()[0]).toMatchObject({
          name: "DeckLog AB12C",
          edition: "jp",
          format: "standard",
          leader: { cardId: "c:sd01-001" },
          main: [
            { cardId: "c:bp01-001", count: 3 },
            { cardId: "c:sd01-002", count: 2 },
          ],
        })
      })
    } finally {
      routeFixtureFetch(undefined)
    }
  }, 60_000)

  it("looks a bare DeckLog code up in the chosen edition and explains a miss", async () => {
    const asked: string[] = []
    routeFixtureFetch((url) => {
      if (!url.startsWith("/api/decklog/")) return undefined
      asked.push(url)
      return Promise.resolve(Response.json({ error: "not-found" }, { status: 404 }))
    })
    try {
      await openImport()
      const user = userEvent.setup()
      fireEvent.change(screen.getByRole("textbox", { name: "牌組文字、分享碼或 DeckLog 代碼" }), {
        target: { value: "zz99z" },
      })
      const editions = await screen.findByRole("radiogroup", { name: "卡片版本" })
      await user.click(within(editions).getByRole("radio", { name: "英版" }))
      await user.click(screen.getByRole("button", { name: "從 DeckLog 讀取" }))
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "英版 DeckLog 找不到代碼 ZZ99Z；如果是另一版的代碼，請切換版本再試。",
      )
      expect(asked).toEqual(["/api/decklog/en/ZZ99Z"])
      // The text is not also read as a deck list.
      expect(screen.queryByRole("region", { name: "辨識結果" })).not.toBeInTheDocument()
      expect(screen.getByRole("button", { name: "建立牌組" })).toBeDisabled()
    } finally {
      routeFixtureFetch(undefined)
    }
  }, 60_000)

  it("lets only the latest DeckLog lookup settle the page", async () => {
    const answers = new Map<string, (response: Response) => void>()
    routeFixtureFetch((url) => {
      if (!url.startsWith("/api/decklog/")) return undefined
      return new Promise<Response>((resolve) => {
        answers.set(url, resolve)
      })
    })
    try {
      await openImport()
      const user = userEvent.setup()
      const box = screen.getByRole("textbox", { name: "牌組文字、分享碼或 DeckLog 代碼" })
      fireEvent.change(box, { target: { value: "https://decklog.bushiroad.com/view/AAAA1" } })
      await user.click(await screen.findByRole("button", { name: "從 DeckLog 讀取" }))
      fireEvent.change(box, { target: { value: "https://decklog.bushiroad.com/view/BBBB2" } })
      await user.click(screen.getByRole("button", { name: "從 DeckLog 讀取" }))
      expect([...answers.keys()]).toEqual(["/api/decklog/jp/AAAA1", "/api/decklog/jp/BBBB2"])

      // The first lookup, already left behind, finishes late with an error.
      answers.get("/api/decklog/jp/AAAA1")?.(
        Response.json({ error: "server-error" }, { status: 500 }),
      )
      await act(async () => {
        await Promise.resolve()
      })
      expect(screen.getByRole("status")).toHaveTextContent("正在查詢 DeckLog…")
      expect(screen.getByRole("button", { name: "從 DeckLog 讀取" })).toBeDisabled()
      expect(screen.queryByRole("alert")).not.toBeInTheDocument()

      answers.get("/api/decklog/jp/BBBB2")?.(
        Response.json({
          region: "jp",
          code: "BBBB2",
          format: "N",
          className: null,
          leader: [],
          main: [{ cardNo: "BP01-001", count: 3 }],
          evolve: [],
        }),
      )
      const preview = await screen.findByRole("region", { name: "辨識結果" }, { timeout: 25_000 })
      expect(within(preview).getByText("來源：DeckLog BBBB2")).toBeInTheDocument()

      // A lookup left before it settles is dropped; coming back to its code allows asking again.
      fireEvent.change(box, { target: { value: "https://decklog.bushiroad.com/view/CCCC3" } })
      await user.click(screen.getByRole("button", { name: "從 DeckLog 讀取" }))
      expect(screen.getByRole("status")).toHaveTextContent("正在查詢 DeckLog…")
      fireEvent.change(box, { target: { value: "https://decklog.bushiroad.com/view/DDDD4" } })
      fireEvent.change(box, { target: { value: "https://decklog.bushiroad.com/view/CCCC3" } })
      expect(screen.getByRole("button", { name: "從 DeckLog 讀取" })).toBeEnabled()
      expect(screen.getByRole("status")).toHaveTextContent("")
    } finally {
      routeFixtureFetch(undefined)
    }
  }, 60_000)
})
