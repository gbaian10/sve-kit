import { screen, within } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { renderRoutes } from "../../test-utils"
import { BottomNav } from "./BottomNav"

const routes = [{ path: "*", element: <BottomNav /> }]

describe("BottomNav", () => {
  it("lights the cards tab for a nested card route and nothing else", async () => {
    await renderRoutes(routes, { initialEntries: ["/cards/BP01-001"] })
    const nav = screen.getByRole("navigation")
    const links = within(nav).getAllByRole("link")
    expect(links.map((link) => link.textContent)).toEqual(["首頁", "查卡", "牌組"])
    expect(within(nav).getByRole("link", { name: "查卡" })).toHaveAttribute("aria-current", "page")
    expect(within(nav).getByRole("link", { name: "首頁" })).not.toHaveAttribute("aria-current")
  })

  it("lights home only on the exact root path", async () => {
    await renderRoutes(routes, { initialEntries: ["/"] })
    expect(screen.getByRole("link", { name: "首頁" })).toHaveAttribute("aria-current", "page")
    expect(screen.getByRole("link", { name: "查卡" })).not.toHaveAttribute("aria-current")
  })

  it("hides while the on-screen keyboard is open", async () => {
    const viewport = new EventTarget() as VisualViewport
    Object.defineProperty(viewport, "height", { value: 300, configurable: true })
    Object.defineProperty(window, "visualViewport", { value: viewport, configurable: true })
    Object.defineProperty(window, "innerHeight", { value: 800, configurable: true })
    try {
      await renderRoutes(routes, { initialEntries: ["/"] })
      expect(screen.queryByRole("navigation")).not.toBeInTheDocument()
    } finally {
      Object.defineProperty(window, "visualViewport", { value: null, configurable: true })
    }
  })
})
