import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it } from "vitest"

import { HelpTip } from "./HelpTip"

const tip = () => screen.getByRole("button", { name: "說明" })

function renderTip() {
  return render(
    <>
      <HelpTip label="說明">範例內容</HelpTip>
      <button type="button">旁邊</button>
    </>,
  )
}

describe("HelpTip", () => {
  it("renders its content only while open and links it to the button", async () => {
    renderTip()
    expect(screen.queryByText("範例內容")).toBeNull()
    expect(tip()).toHaveAttribute("aria-expanded", "false")
    expect(tip()).not.toHaveAttribute("aria-describedby")
    await userEvent.hover(tip())
    expect(screen.getByRole("tooltip")).toHaveTextContent("範例內容")
    expect(tip()).toHaveAttribute("aria-expanded", "true")
    expect(tip()).toHaveAttribute("aria-describedby", screen.getByRole("tooltip").id)
    await userEvent.unhover(tip())
    expect(screen.queryByRole("tooltip")).toBeNull()
  })

  it("opens on keyboard focus without taking focus, and Esc closes it", async () => {
    renderTip()
    await userEvent.tab()
    expect(tip()).toHaveFocus()
    expect(screen.getByRole("tooltip")).toBeInTheDocument()
    await userEvent.keyboard("{Escape}")
    expect(screen.queryByRole("tooltip")).toBeNull()
    expect(tip()).toHaveFocus()
    await userEvent.tab()
    expect(screen.queryByRole("tooltip")).toBeNull()
  })

  it("pins on click, closes on a second click, and closes on a tap outside", async () => {
    renderTip()
    await userEvent.click(tip())
    expect(screen.getByRole("tooltip")).toBeInTheDocument()
    await userEvent.click(tip())
    expect(screen.queryByRole("tooltip")).toBeNull()
    await userEvent.click(tip())
    await userEvent.unhover(tip())
    expect(screen.getByRole("tooltip")).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: "旁邊" }))
    expect(screen.queryByRole("tooltip")).toBeNull()
  })

  it("closes on a second click after keyboard focus opened it", async () => {
    renderTip()
    await userEvent.tab()
    expect(screen.getByRole("tooltip")).toBeInTheDocument()
    await userEvent.click(tip())
    expect(screen.getByRole("tooltip")).toBeInTheDocument()
    await userEvent.click(tip())
    expect(screen.queryByRole("tooltip")).toBeNull()
    expect(tip()).toHaveFocus()
  })
})
