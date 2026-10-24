import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { createRef, useRef, useState } from "react"
import { expect, it, vi } from "vitest"

import { CompactHeaderMenu } from "./CompactHeaderMenu"

it("keeps its actions usable and dismisses with Escape before the surrounding drawer", async () => {
  const action = vi.fn()
  const surroundingKey = vi.fn()
  window.addEventListener("keydown", surroundingKey)
  try {
    render(
      <CompactHeaderMenu label="Current conditions" icon={null} fallbackFocusRef={createRef()}>
        <button type="button" onClick={action}>
          Clear conditions
        </button>
      </CompactHeaderMenu>,
    )
    const user = userEvent.setup()
    const trigger = screen.getByRole("button", { name: "Current conditions" })
    await user.click(trigger)
    const clear = screen.getByRole("button", { name: "Clear conditions" })
    expect(trigger).toHaveAttribute("aria-expanded", "true")
    expect(clear).toHaveFocus()
    await user.click(clear)
    expect(action).toHaveBeenCalledOnce()
    await user.keyboard("{Escape}")
    expect(trigger).toHaveAttribute("aria-expanded", "false")
    expect(trigger).toHaveFocus()
    expect(surroundingKey).not.toHaveBeenCalled()
  } finally {
    window.removeEventListener("keydown", surroundingKey)
  }
})

it("returns focus to search when removing the last condition removes the menu", async () => {
  function Harness() {
    const search = useRef<HTMLInputElement>(null)
    const [hasConditions, setHasConditions] = useState(true)
    return (
      <>
        <input ref={search} aria-label="Search" />
        {hasConditions && (
          <CompactHeaderMenu label="Current conditions" icon={null} fallbackFocusRef={search}>
            <button
              type="button"
              onClick={() => {
                setHasConditions(false)
              }}
            >
              Remove last condition
            </button>
          </CompactHeaderMenu>
        )}
      </>
    )
  }
  render(<Harness />)
  const user = userEvent.setup()
  await user.click(screen.getByRole("button", { name: "Current conditions" }))
  await user.click(screen.getByRole("button", { name: "Remove last condition" }))
  expect(screen.getByRole("textbox", { name: "Search" })).toHaveFocus()
})
