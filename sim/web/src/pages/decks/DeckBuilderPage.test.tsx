import "fake-indexeddb/auto"

import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { routes } from "../../app/routes"
import { createDeck } from "../../domain/deck/model"
import type { Deck } from "../../domain/deck/types"
import { decksStore, DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestAppWorker } from "../../test-utils/app-workers"

beforeEach(() => vi.stubGlobal("Worker", TestAppWorker))

const built = await buildSnapshot({
  encodeImage: () => Promise.resolve(new TextEncoder().encode("stub")),
})

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
afterEach(async () => {
  prefsStore.set(DEFAULT_PREFS)
  await decksStore.reload()
  const state = decksStore.get()
  if (state.status === "ready") for (const deck of state.decks) await decksStore.remove(deck.id)
})

async function openBuilder(classCode = "royal") {
  const deck = createDeck(
    { name: "王族測試", edition: "jp", basis: { kind: "class", classCode } },
    new Date("2026-11-03T00:00:00.000Z"),
  )
  await decksStore.save(deck)
  prefsStore.set({ uiLanguage: "zh-TW", viewMode: "grid" })
  const { router } = await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
  return { ...deck, router }
}

function stored(id: string): Deck | undefined {
  const state = decksStore.get()
  return state.status === "ready" ? state.decks.find((deck) => deck.id === id) : undefined
}

/** A window whose width the test changes: the 1180px query matches once `wide` is set. */
function resizableWindow() {
  const listeners = new Set<() => void>()
  let wide = false
  const media = vi.spyOn(window, "matchMedia").mockImplementation(
    (query: string) =>
      ({
        matches: wide && query.includes("1180px"),
        media: query,
        onchange: null,
        addEventListener: (_type: string, listener: () => void) => listeners.add(listener),
        removeEventListener: (_type: string, listener: () => void) => listeners.delete(listener),
        addListener: () => undefined,
        removeListener: () => undefined,
        dispatchEvent: () => false,
      }) as unknown as MediaQueryList,
  )
  onTestFinished(() => {
    media.mockRestore()
  })
  return (next: boolean) => {
    wide = next
    for (const listener of listeners) listener()
  }
}

describe("DeckBuilderPage", () => {
  it("adds copies past the same-name limit, warns about it and lists them in the deck", async () => {
    const deck = await openBuilder()
    const user = userEvent.setup()
    // The steppers appear once the pool's results and the deck's card facts have both loaded.
    await screen.findAllByRole("button", { name: "加一張 試作の見習い兵" }, { timeout: 25_000 })
    const pool = () => document.querySelector<HTMLElement>("[data-card-pool]") ?? document.body
    const add = () => within(pool()).getByRole("button", { name: "加一張 試作の見習い兵" })
    await user.click(add())
    // The rule's limit stays the denominator however many copies are in.
    expect(add().parentElement).toHaveTextContent("1/3")
    // Construction rules only warn: a fourth copy still goes in (maintainer decision 2026-11-03).
    for (let copy = 1; copy < 4; copy += 1) await user.click(add())
    expect(add()).toBeEnabled()
    expect(stored(deck.id)?.main).toEqual([{ cardId: "c:bp01-001", count: 4 }])
    // Below 1180 px the deck is a sheet over the pool.
    await user.click(screen.getByRole("button", { name: "打開牌組" }))
    const list = await screen.findByRole("region", { name: "主牌組" })
    expect(within(list).getByRole("button", { name: "試作の見習い兵" })).toBeInTheDocument()
    expect(
      await screen.findByText("主牌組有 4 張「試作の見習い兵」，同名最多 3 張。"),
    ).toBeInTheDocument()

    await user.click(within(list).getByRole("button", { name: "減一張 試作の見習い兵" }))
    expect(stored(deck.id)?.main).toEqual([{ cardId: "c:bp01-001", count: 3 }])
    expect(screen.getByText("主牌組 3 張，要 40～50 張。")).toBeInTheDocument()
    expect(screen.queryByText(/同名最多/u)).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "繼續加卡" }))
    expect(screen.queryByRole("dialog", { name: "牌組" })).not.toBeInTheDocument()
  }, 60_000)

  it("sets the leader from the pool and keeps other classes out by default", async () => {
    const deck = await openBuilder()
    const user = userEvent.setup()
    await user.click(
      await screen.findByRole("button", { name: "把 試作の王 設為主戰者" }, { timeout: 25_000 }),
    )
    await waitFor(() => {
      expect(stored(deck.id)?.leader).toEqual({ cardId: "c:sd01-001" })
    })
    // The pool shows the royal deck's cards and neutral ones only.
    expect(screen.queryByRole("button", { name: "加一張 試作の妖精" })).not.toBeInTheDocument()
  }, 60_000)

  it("says so when the deck does not exist", async () => {
    prefsStore.set({ uiLanguage: "zh-TW" })
    await renderRoutes(routes, { initialEntries: ["/decks/missing"] })
    expect(await screen.findByText("找不到這副牌組")).toBeInTheDocument()
  })

  it("offers leaders of every class to a neutral deck", async () => {
    await openBuilder("neutral")
    const user = userEvent.setup()
    await user.click(await screen.findByRole("radio", { name: "主戰者" }, { timeout: 25_000 }))
    expect(
      await screen.findByRole("button", { name: "把 試作の王 設為主戰者" }, { timeout: 25_000 }),
    ).toBeInTheDocument()
  }, 60_000)

  it("keeps the pool one cell per card: its filters offer no result unit", async () => {
    await openBuilder()
    const user = userEvent.setup()
    await screen.findAllByRole("button", { name: "加一張 試作の見習い兵" }, { timeout: 25_000 })
    await user.click(screen.getByRole("button", { name: /^篩選/u }))
    expect(await screen.findByRole("combobox", { name: "代表版次" })).toBeInTheDocument()
    expect(screen.queryByRole("combobox", { name: "顯示方式" })).not.toBeInTheDocument()
  }, 60_000)

  it("puts the deck behind a summary bar below 1180 px; back closes the deck sheet", async () => {
    const { router, id } = await openBuilder()
    const user = userEvent.setup()
    const open = await screen.findByRole("button", { name: "打開牌組" }, { timeout: 25_000 })
    expect(open).toHaveTextContent("王族測試")
    await user.click(open)
    expect(await screen.findByRole("dialog", { name: "牌組" })).toBeInTheDocument()
    await act(async () => {
      await router.navigate(-1)
    })
    expect(screen.queryByRole("dialog", { name: "牌組" })).not.toBeInTheDocument()
    expect(router.state.location.pathname).toBe(`/decks/${id}`)
  }, 60_000)

  it("closes the sheet and the dialog opened in it, with their back entries, on widening to three columns", async () => {
    const setWide = resizableWindow()
    const { router, id } = await openBuilder()
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "打開牌組" }, { timeout: 25_000 }))
    const sheet = await screen.findByRole("dialog", { name: "牌組" })
    await user.click(await within(sheet).findByRole("button", { name: "匯出／分享" }))
    expect(await screen.findByRole("dialog", { name: "匯出／分享" })).toBeInTheDocument()
    expect(document.documentElement.style.overflow).toBe("hidden")

    await act(async () => {
      setWide(true)
      await Promise.resolve()
    })
    await waitFor(() => {
      expect(router.state.location.state).toBeNull()
    })
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(document.documentElement.style.overflow).toBe("")
    expect(screen.getByRole("complementary", { name: "牌組" })).toBeInTheDocument()
    expect(router.state.location.pathname).toBe(`/decks/${id}`)

    await act(async () => {
      setWide(false)
      await Promise.resolve()
    })
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
  }, 60_000)

  it("also drops a card opened from the sheet when the layout widens", async () => {
    const setWide = resizableWindow()
    const deck = createDeck(
      { name: "單卡", edition: "jp", basis: { kind: "class", classCode: "royal" } },
      new Date("2026-11-03T00:00:00.000Z"),
    )
    await decksStore.save({ ...deck, main: [{ cardId: "c:bp01-001", count: 1 }] })
    prefsStore.set({ uiLanguage: "zh-TW", viewMode: "grid" })
    const { router } = await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
    const user = userEvent.setup()
    await user.click(await screen.findByRole("button", { name: "打開牌組" }, { timeout: 25_000 }))
    const list = await screen.findByRole("region", { name: "主牌組" }, { timeout: 25_000 })
    await user.click(
      await within(list).findByRole("button", { name: "試作の見習い兵" }, { timeout: 25_000 }),
    )
    await waitFor(() => {
      expect(router.state.location.search).toContain("card=")
    })

    await act(async () => {
      setWide(true)
      await Promise.resolve()
    })
    await waitFor(() => {
      expect(router.state.location.state).toBeNull()
    })
    expect(router.state.location.pathname + router.state.location.search).toBe(`/decks/${deck.id}`)
  }, 60_000)
})
