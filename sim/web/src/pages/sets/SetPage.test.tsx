import { screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { stubFixtureFetch } from "../../test-utils/fixture-fetch"
import { TestSearchWorker } from "../../test-utils/search-worker"

beforeEach(() => vi.stubGlobal("Worker", TestSearchWorker))

await stubFixtureFetch()

afterAll(() => {
  vi.unstubAllGlobals()
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
})

function headerOf(heading: HTMLElement) {
  const header = heading.closest("header")
  if (!header) throw new Error("heading outside the set header")
  return within(header)
}

async function open(path: string) {
  prefsStore.set({ uiLanguage: "zh-TW", filterPanelOpen: false })
  return renderRoutes(routes, { initialEntries: [path] })
}

describe("SetPage header", () => {
  it("names the archive family with its kind, code, release date and card count", async () => {
    await open("/sets/BP01")
    // Until the set list arrives the heading is the path code; the family name replaces it.
    const heading = await screen.findByRole(
      "heading",
      { level: 1, name: "試作補充包第1彈" },
      { timeout: 4000 },
    )
    expect(heading).toHaveAttribute("lang", "zh-Hant")
    const header = headerOf(heading)
    // The meta line above the heading: kind, code, then the edition's first dated release.
    await waitFor(() => {
      expect(heading.previousElementSibling).toHaveTextContent("補充包・BP01・2026-02-01 發售")
    })
    expect(await header.findByText("已收錄 21 種", {}, { timeout: 4000 })).toBeVisible()
    expect(header.getByRole("link", { name: "全部卡包" })).toHaveAttribute("href", "/sets")
  })

  it("follows ?ed=en: English release date, English counts and English links", async () => {
    await open("/sets/BP01?ed=en")
    const heading = await screen.findByRole(
      "heading",
      { level: 1, name: "試作補充包第1彈" },
      { timeout: 4000 },
    )
    const header = headerOf(heading)
    await waitFor(() => {
      expect(heading.previousElementSibling).toHaveTextContent("補充包・BP01・2026-04-01 發售")
    })
    expect(await header.findByText("已收錄 15 種", {}, { timeout: 4000 })).toBeVisible()
    expect(header.getByRole("link", { name: "全部卡包" })).toHaveAttribute("href", "/sets?ed=en")
  })

  it("does not list the page's own set as a removable condition", async () => {
    await open("/sets/BP01?class=bishop")
    await screen.findByRole("heading", { level: 1, name: "試作補充包第1彈" }, { timeout: 4000 })
    const chips = screen
      .getAllByRole("button", { name: /^移除條件：/u })
      .map((button) => button.getAttribute("aria-label"))
    expect(chips).toEqual(["移除條件：主教"])
    // Only the class is set, and the class row is not counted: the filter button shows no badge.
    expect(screen.getByRole("button", { name: "篩選" })).toHaveTextContent(/^篩選$/u)
  })

  it("keeps the implied set in a multi-select when another chip is removed", async () => {
    const { router } = await open("/sets/BP01?set=bp01,sd01&class=elf")
    await screen.findByRole("heading", { level: 1, name: "試作補充包第1彈" }, { timeout: 4000 })
    const labels = () =>
      screen
        .getAllByRole("button", { name: /^移除條件：/u })
        .map((button) => button.getAttribute("aria-label"))
    expect(labels()).toEqual(["移除條件：精靈", "移除條件：SD01 試作起始牌組"])
    await userEvent.click(screen.getByRole("button", { name: "移除條件：精靈" }))
    expect(router.state.location.search).toBe("?set=bp01%2Csd01")
    expect(labels()).toEqual(["移除條件：SD01 試作起始牌組"])
  })

  it("labels a deck family by its own kind", async () => {
    await open("/sets/SD01")
    const heading = await screen.findByRole(
      "heading",
      { level: 1, name: "試作起始牌組" },
      { timeout: 4000 },
    )
    await waitFor(() => {
      expect(heading.previousElementSibling).toHaveTextContent("牌組・SD01・2026-02-01 發售")
    })
  })

  it("offers the other edition when the code has no printings in this one", async () => {
    // The promo set exists in JP only, so under EN it is not found rather than shown empty.
    prefsStore.set({ cardEdition: "en" })
    const { router } = await open("/sets/PR?ed=en")
    const text = await screen.findByText("找不到這個卡包", {}, { timeout: 4000 })
    const notice = text.closest<HTMLElement>("[role=status]")
    if (!notice) throw new Error("not-found text outside its notice")
    expect(within(notice).getByRole("link", { name: "改看日版" })).toHaveAttribute(
      "href",
      "/sets/PR",
    )
    expect(within(notice).getByRole("link", { name: "全部卡包" })).toHaveAttribute(
      "href",
      "/sets?ed=en",
    )
    expect(document.querySelector("[data-result-key]")).toBeNull()
    // Choosing the other edition here is a preference too, unlike merely opening a URL.
    expect(prefsStore.get().cardEdition).toBe("en")
    await userEvent.click(within(notice).getByRole("link", { name: "改看日版" }))
    expect(router.state.location.pathname).toBe("/sets/PR")
    expect(router.state.location.search).toBe("")
    expect(prefsStore.get().cardEdition).toBe("jp")
    await screen.findByRole("heading", { level: 1, name: "試作促銷卡" }, { timeout: 4000 })
    await waitFor(() => {
      expect(document.querySelector("[data-result-key]")).not.toBeNull()
    })
  })
})
