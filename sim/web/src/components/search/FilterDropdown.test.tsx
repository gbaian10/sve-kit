import { fireEvent, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
import { FilterDropdown } from "./FilterDropdown"

it("keeps the open menu aligned to its trigger when scrolling leaves room only above", async () => {
  await renderInRouter(
    <FilterDropdown label="類型">
      <input type="checkbox" aria-label="妖精" />
    </FilterDropdown>,
  )
  const trigger = screen.getByRole("button", { name: /^類型:\s*全部$/u })
  const popup = document.getElementById(trigger.getAttribute("aria-controls") ?? "")
  if (!popup) throw new Error("Missing controlled popover")
  let anchor = new DOMRect(240, 100, 120, 44)
  vi.spyOn(trigger, "getBoundingClientRect").mockImplementation(() => anchor)
  Object.defineProperty(popup, "scrollHeight", { value: 180, configurable: true })
  vi.spyOn(popup, "getBoundingClientRect").mockImplementation(() => new DOMRect(0, 0, 480, 180))
  try {
    await userEvent.click(trigger)
    expect(trigger).toHaveAttribute("aria-expanded", "true")
    expect(popup.style.left).toBe("240px")
    expect(popup.style.top).toBe("150px")

    anchor = new DOMRect(410, window.innerHeight - 100, 120, 44)
    fireEvent.scroll(window)
    expect(trigger).toHaveAttribute("aria-expanded", "true")
    expect(popup.style.left).toBe("410px")
    expect(popup.style.top).toBe(`${String(window.innerHeight - 286)}px`)
  } finally {
    vi.restoreAllMocks()
  }
})
