import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { stubFixtureFetch } from "../../test-utils/fixture-fetch"
import { TestSearchWorker } from "../../test-utils/search-worker"

beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))

// The synthetic snapshot has three families: a booster and a starter deck in both editions and a
// JP-only promo set. Only the booster and the deck have dated products.
await stubFixtureFetch()

afterAll(() => {
  vi.unstubAllGlobals()
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

/** The section a kind heading belongs to. */
function section(name: string) {
  const parent = screen.getByRole("heading", { level: 2, name }).parentElement
  if (!parent) throw new Error(`missing section for ${name}`)
  return within(parent)
}

async function open(path: string) {
  prefsStore.set({ uiLanguage: "zh-TW" })
  const rendered = await renderRoutes(routes, { initialEntries: [path] })
  await screen.findByRole("heading", { level: 2, name: "補充包" }, { timeout: 4000 })
  return rendered
}

describe("SetsPage", () => {
  it("groups the edition's families by kind with their release date and printing count", async () => {
    await open("/sets")
    const boosters = section("補充包")
    const booster = boosters.getByRole("link", { name: /BP01/u })
    expect(booster).toHaveAttribute("href", "/cards?set=bp01")
    expect(booster).toHaveTextContent("試作補充包第1彈")
    expect(booster.querySelector("[lang]")).toHaveAttribute("lang", "zh-Hant")
    await waitFor(() => {
      expect(booster).toHaveTextContent(/2026-02-01 發售・\d+ 個版次/u)
    })
    // The promo set has no dated product: it is listed with its count and no date.
    const promos = section("PR")
    const promo = promos.getByRole("link", { name: /^PR/u })
    expect(promo).toHaveTextContent("試作促銷卡")
    expect(promo).not.toHaveTextContent("發售")
    expect(promo).toHaveTextContent(/\d+ 個版次/u)
    // Decks and other kinds sit behind one collapsed disclosure.
    const details = screen.getByText("牌組與特別商品").closest("details")
    if (!details) throw new Error("missing disclosure")
    expect(details).not.toHaveAttribute("open")
    expect(within(details).getByRole("link", { name: /SD01/u })).toHaveTextContent("試作起始牌組")
    expect(screen.queryByRole("heading", { level: 2, name: "合作包" })).not.toBeInTheDocument()
  })

  it("lists only the English edition's families under ?ed=en and keeps the edition on each link", async () => {
    await open("/sets?ed=en")
    const booster = screen.getByRole("link", { name: /BP01/u })
    expect(booster).toHaveAttribute("href", "/cards?set=bp01&ed=en")
    await waitFor(() => {
      expect(booster).toHaveTextContent("2026-04-01 發售")
    })
    expect(screen.queryByRole("link", { name: /^PR/u })).not.toBeInTheDocument()
    expect(screen.getByRole("radio", { name: "英版" })).toBeChecked()
  })

  it("switches the edition through the URL and remembers it as the card edition", async () => {
    const { router } = await open("/sets")
    await userEvent.click(screen.getByRole("radio", { name: "英版" }))
    expect(router.state.location.pathname).toBe("/sets")
    expect(router.state.location.search).toBe("?ed=en")
    expect(prefsStore.get().cardEdition).toBe("en")
    await userEvent.click(screen.getByRole("radio", { name: "日版" }))
    expect(router.state.location.search).toBe("")
    expect(prefsStore.get().cardEdition).toBe("jp")
  })
})
