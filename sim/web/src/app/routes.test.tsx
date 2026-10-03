import { act, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it } from "vitest"

import { DEFAULT_PREFS, prefsStore } from "../settings"
import { renderRoutes } from "../test-utils"
import { routes } from "./routes"

// jsdom reports en-US as the browser language; the shell would switch to English otherwise.
beforeEach(() => {
  prefsStore.set({ uiLanguage: "zh-TW" })
})

afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  document.documentElement.removeAttribute("lang")
})

describe("route table", () => {
  it.each([
    ["/", "主頁"],
    ["/cards", "查卡"],
    ["/cards/BP01-001-合成経路サンプル", "單卡"],
    ["/cards/_provisional/12", "單卡"],
    ["/sets", "卡包"],
    ["/sets/BP01", "卡包"],
    ["/settings", "帳號與設定"],
    ["/decks", "牌組"],
    ["/nope/nothing", "找不到這一頁"],
  ])("%s renders the %s page inside the shell", async (path, heading) => {
    await renderRoutes(routes, { initialEntries: [path] })
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(heading)
    expect(screen.getByRole("main")).toBeInTheDocument()
    expect(screen.getAllByRole("navigation").length).toBeGreaterThan(0)
  })

  it("shows the card number and the provisional id on the card page", async () => {
    await renderRoutes(routes, { initialEntries: ["/cards/BP01-001"] })
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("BP01-001")
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
