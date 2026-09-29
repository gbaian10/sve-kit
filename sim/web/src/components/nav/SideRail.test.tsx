import { screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { renderRoutes } from "../../test-utils"
import { SideRail } from "./SideRail"

function Harness() {
  return <SideRail />
}

describe("SideRail", () => {
  it("stays a fixed 72px rail and marks only the active item", async () => {
    await renderRoutes([{ path: "*", element: <Harness /> }], { initialEntries: ["/sets"] })
    expect(screen.queryByRole("button", { name: /側欄/u })).not.toBeInTheDocument()
    for (const name of ["首頁", "查卡", "牌組"]) {
      expect(screen.getByRole("link", { name })).not.toHaveAttribute("aria-current")
    }
    const aside = screen.getByRole("complementary")
    expect(aside.className).toContain("w-18")
    expect(aside.className).not.toMatch(/w-50/u)
  })

  it("exposes the logo link and leaves the account menu to the top bar", async () => {
    await renderRoutes([{ path: "*", element: <Harness /> }], { initialEntries: ["/cards"] })
    expect(screen.getByRole("link", { name: "sve-kit" })).toHaveAttribute("href", "/")
    expect(screen.getByRole("link", { name: "查卡" })).toHaveAttribute("aria-current", "page")
    expect(screen.queryByRole("button", { name: "帳號與設定" })).not.toBeInTheDocument()
  })
})
