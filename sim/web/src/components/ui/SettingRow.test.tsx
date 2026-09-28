import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { SettingRow } from "./SettingRow"

describe("SettingRow", () => {
  it("renders the label, the hint and the control", () => {
    render(
      <SettingRow label="Data saver" hint="Images load on tap">
        <button type="button">control</button>
      </SettingRow>,
    )
    expect(screen.getByText("Data saver")).toBeInTheDocument()
    expect(screen.getByText("Images load on tap")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "control" })).toBeInTheDocument()
  })

  it("lets the control wrap under the label instead of shrinking either side", () => {
    const { container } = render(
      <SettingRow label="Label">
        <span>control</span>
      </SettingRow>,
    )
    const row = container.firstElementChild
    expect(row).toHaveClass("flex-wrap")
    expect(screen.getByText("control").parentElement).toHaveClass("shrink-0", "ml-auto")
  })

  it("puts the hint on its own full-width line", () => {
    render(
      <SettingRow label="Data saver" hint="Images load on tap">
        <span>control</span>
      </SettingRow>,
    )
    expect(screen.getByText("Images load on tap")).toHaveClass("basis-full")
  })
})
