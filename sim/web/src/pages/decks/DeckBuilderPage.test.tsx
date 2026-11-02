import "fake-indexeddb/auto"

import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest"

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

async function openBuilder(classCode = "royal"): Promise<Deck> {
  const deck = createDeck(
    { name: "王族測試", edition: "jp", basis: { kind: "class", classCode } },
    new Date("2026-11-03T00:00:00.000Z"),
  )
  await decksStore.save(deck)
  prefsStore.set({ uiLanguage: "zh-TW", viewMode: "grid" })
  await renderRoutes(routes, { initialEntries: [`/decks/${deck.id}`] })
  return deck
}

function stored(id: string): Deck | undefined {
  const state = decksStore.get()
  return state.status === "ready" ? state.decks.find((deck) => deck.id === id) : undefined
}

describe("DeckBuilderPage", () => {
  it("adds copies from the pool until the same-name limit and lists them in the deck", async () => {
    const deck = await openBuilder()
    const user = userEvent.setup()
    // The steppers appear once the pool's results and the deck's card facts have both loaded.
    await screen.findAllByRole("button", { name: "加一張 試作の見習い兵" }, { timeout: 15_000 })
    const pool = () => document.querySelector<HTMLElement>("[data-card-pool]") ?? document.body
    const add = () => within(pool()).getByRole("button", { name: "加一張 試作の見習い兵" })
    await user.click(add())
    // The rule's limit stays the denominator however many copies are in.
    expect(add().parentElement).toHaveTextContent("1/3")
    for (let copy = 1; copy < 3; copy += 1) await user.click(add())
    expect(add()).toBeDisabled()
    expect(stored(deck.id)?.main).toEqual([{ cardId: "c:bp01-001", count: 3 }])
    const list = screen.getByRole("region", { name: "主牌組" })
    expect(within(list).getByRole("button", { name: "試作の見習い兵" })).toBeInTheDocument()
    expect(screen.getByText("主牌組 3 張，要 40～50 張。")).toBeInTheDocument()

    await user.click(within(list).getByRole("button", { name: "減一張 試作の見習い兵" }))
    expect(stored(deck.id)?.main).toEqual([{ cardId: "c:bp01-001", count: 2 }])
    expect(add()).toBeEnabled()
  }, 45_000)

  it("sets the leader from the pool and keeps other classes out by default", async () => {
    const deck = await openBuilder()
    const user = userEvent.setup()
    await user.click(
      await screen.findByRole("button", { name: "把 試作の王 設為主戰者" }, { timeout: 15_000 }),
    )
    await waitFor(() => {
      expect(stored(deck.id)?.leader).toEqual({ cardId: "c:sd01-001" })
    })
    // The pool shows the royal deck's cards and neutral ones only.
    expect(screen.queryByRole("button", { name: "加一張 試作の妖精" })).not.toBeInTheDocument()
  }, 45_000)

  it("says so when the deck does not exist", async () => {
    prefsStore.set({ uiLanguage: "zh-TW" })
    await renderRoutes(routes, { initialEntries: ["/decks/missing"] })
    expect(await screen.findByText("找不到這副牌組")).toBeInTheDocument()
  })

  it("offers leaders of every class to a neutral deck", async () => {
    await openBuilder("neutral")
    const user = userEvent.setup()
    await user.click(await screen.findByRole("radio", { name: "主戰者" }, { timeout: 15_000 }))
    expect(
      await screen.findByRole("button", { name: "把 試作の王 設為主戰者" }, { timeout: 15_000 }),
    ).toBeInTheDocument()
  }, 45_000)

  it("keeps the pool one cell per card: its filters offer no result unit", async () => {
    await openBuilder()
    const user = userEvent.setup()
    await screen.findAllByRole("button", { name: "加一張 試作の見習い兵" }, { timeout: 15_000 })
    await user.click(screen.getByRole("button", { name: /^篩選/u }))
    expect(await screen.findByRole("combobox", { name: "代表版次" })).toBeInTheDocument()
    expect(screen.queryByRole("combobox", { name: "顯示方式" })).not.toBeInTheDocument()
  }, 45_000)
})
