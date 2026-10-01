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

  it.each(["/cards/BP01-001/リノセウス", "/cards/_provisional/12"])(
    "%s renders the card page inside the shell and reports the missing snapshot",
    async (path) => {
      await renderRoutes(routes, { initialEntries: [path] })
      expect(screen.getByRole("main")).toBeInTheDocument()
      // No snapshot can load in this test, so the card page ends in its failure state.
      expect(await screen.findByRole("alert")).toHaveTextContent("卡片資料載入失敗")
    },
  )

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
