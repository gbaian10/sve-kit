import { fireEvent, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
import { FilterDropdown } from "./FilterDropdown"

it("closes when scrolling instead of leaving the menu detached from its trigger", async () => {
  await renderInRouter(
    <FilterDropdown label="類型">
      <input type="checkbox" aria-label="妖精" />
    </FilterDropdown>,
  )
  const trigger = screen.getByRole("button", { name: /^類型:\s*全部$/u })
  const popup = document.getElementById(trigger.getAttribute("aria-controls") ?? "")
  if (!popup) throw new Error("Missing controlled popover")
  const anchor = new DOMRect(240, 100, 120, 44)
  vi.spyOn(trigger, "getBoundingClientRect").mockImplementation(() => anchor)
  Object.defineProperty(popup, "scrollHeight", { value: 180, configurable: true })
  vi.spyOn(popup, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, 0, 480, 180))
  try {
    await userEvent.click(trigger)
    expect(trigger).toHaveAttribute("aria-expanded", "true")
    expect(popup.style.left).toBe("240px")
    expect(popup.style.top).toBe("150px")

    fireEvent.scroll(window)
    expect(trigger).toHaveAttribute("aria-expanded", "false")
    expect(popup.matches(":popover-open")).toBe(false)
  } finally {
    vi.restoreAllMocks()
  }
})

it("fits the panel bounds and aligns the menu inward at the trailing edge", async () => {
  const { container } = await renderInRouter(
    <div style={{ height: 600, overflowY: "auto" }}>
      <FilterDropdown label="Token" compact>
        <input type="checkbox" aria-label="Follower" />
      </FilterDropdown>
    </div>,
  )
  const boundary = container.querySelector<HTMLElement>("[style]")
  const trigger = screen.getByRole("button", { name: "Token" })
  const popup = document.getElementById(trigger.getAttribute("aria-controls") ?? "")
  if (!boundary || !popup) throw new Error("Missing dropdown layout")
  vi.spyOn(boundary, "getBoundingClientRect").mockReturnValue(new DOMRect(40, 80, 320, 600))
  vi.spyOn(trigger, "getBoundingClientRect").mockReturnValue(new DOMRect(280, 200, 40, 44))

  await userEvent.click(trigger)

  expect(popup.style.width).toBe("296px")
  expect(popup.style.left).toBe("52px")
})
