import { screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { describe, expect, it } from "vitest"

import { renderRoutes } from "../../test-utils"
import { SideRail } from "./SideRail"

function Harness() {
  const [expanded, setExpanded] = useState(false)
  return (
    <SideRail
      expanded={expanded}
      onToggle={() => {
        setExpanded((value) => !value)
      }}
    />
  )
}

describe("SideRail", () => {
  it("toggles between collapsed and expanded and marks the active item", async () => {
    await renderRoutes([{ path: "*", element: <Harness /> }], { initialEntries: ["/sets"] })
    const toggle = screen.getByRole("button", { name: "展開側欄" })
    expect(toggle).toHaveAttribute("aria-expanded", "false")
    await userEvent.click(toggle)
    expect(screen.getByRole("button", { name: "收合側欄" })).toHaveAttribute(
      "aria-expanded",
      "true",
    )
    for (const name of ["首頁", "查卡", "牌組"]) {
      expect(screen.getByRole("link", { name })).not.toHaveAttribute("aria-current")
    }
    // Expanded width only applies from xl; below that the rail must stay 72px or it covers content.
    const aside = screen.getByRole("complementary")
    expect(aside.className).toContain("w-18")
    expect(aside.className).toContain("xl:w-50")
    expect(aside.className).not.toMatch(/(^|\s)w-50(\s|$)/)
  })

  it("exposes the logo link and the account menu", async () => {
    await renderRoutes([{ path: "*", element: <Harness /> }], { initialEntries: ["/cards"] })
    expect(screen.getByRole("link", { name: "sve-kit" })).toHaveAttribute("href", "/")
    expect(screen.getByRole("link", { name: "查卡" })).toHaveAttribute("aria-current", "page")
    expect(screen.getByRole("button", { name: "帳號與設定" })).toBeInTheDocument()
  })
})
