import { act, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { DEFAULT_PREFS, prefsStore } from "../settings"
import { renderRoutes } from "../test-utils"
import { TestSearchWorker } from "../test-utils/search-worker"
import { routes } from "./routes"

// jsdom reports en-US as the browser language; the shell would switch to English otherwise.
beforeEach(() => {
  vi.stubGlobal("Worker", TestSearchWorker)
  prefsStore.set({ uiLanguage: "zh-TW" })
})

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  document.documentElement.removeAttribute("lang")
  vi.unstubAllGlobals()
})

describe("route table", () => {
  it.each([
    ["/", "主頁"],
    ["/cards", "查卡"],
    // The card page's heading is the card number itself (design 06), never a generic title.
    ["/cards/BP01-001-合成経路サンプル", "BP01-001-合成経路サンプル"],
    ["/cards/_provisional/12", "12"],
    ["/sets", "卡包"],
    ["/sets/BP01", "查卡"],
    ["/settings", "帳號與設定"],
    ["/decks", "牌組"],
    ["/nope/nothing", "找不到這一頁"],
  ])("%s renders the %s page inside the shell", async (path, heading) => {
    await renderRoutes(routes, { initialEntries: [path] })
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(heading)
    expect(screen.getByRole("main")).toBeInTheDocument()
    expect(screen.getAllByRole("navigation").length).toBeGreaterThan(0)
  })

  it.each([
    ["/sets/BP01", "?set=bp01"],
    ["/sets/SD01?ed=en&class=elf", "?ed=en&class=elf&set=sd01"],
    ["/sets/BP01?set=sd01&view=list", "?set=bp01&view=list"],
    ["/sets/UNKNOWN", "?set=unknown"],
  ])("redirects the shared set URL %s to its cards filter", async (path, search) => {
    const { router } = await renderRoutes(routes, { initialEntries: [path] })
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/cards")
      expect(router.state.location.search).toBe(search)
    })
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("查卡")
  })

  it("shows the card number and the provisional id on the card page", async () => {
    await renderRoutes(routes, { initialEntries: ["/cards/BP01-001"] })
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("BP01-001")
  })

  it("keeps <html lang> and the UI text in step with the language preference", async () => {
    await renderRoutes(routes, { initialEntries: ["/cards"] })
    expect(document.documentElement.lang).toBe("zh-TW")
    act(() => {
      prefsStore.set({ uiLanguage: "en" })
    })
    expect(await screen.findByRole("heading", { level: 1, name: "Cards" })).toBeInTheDocument()
    expect(document.documentElement.lang).toBe("en")
  })
})
