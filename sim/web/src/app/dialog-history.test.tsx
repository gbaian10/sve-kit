import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { describe, expect, it, vi } from "vitest"

import { renderInRouter } from "../test-utils"
import { useDialogHistory } from "./dialog-history"

function Harness({ onClosed }: { readonly onClosed: () => void }) {
  const [open, setOpen] = useState(false)
  const close = useDialogHistory("filters", open, () => {
    onClosed()
    setOpen(false)
  })
  return (
    <div>
      <button
        type="button"
        onClick={() => {
          setOpen(true)
        }}
      >
        open
      </button>
      {open && (
        <div role="dialog" aria-label="filters">
          <button type="button" onClick={close}>
            close
          </button>
        </div>
      )}
    </div>
  )
}

describe("useDialogHistory", () => {
  it("pushes an entry on open and closes when the browser goes back", async () => {
    const onClosed = vi.fn()
    const { router } = await renderInRouter(<Harness onClosed={onClosed} />, {
      initialEntries: ["/cards?q=a"],
    })
    await userEvent.click(screen.getByRole("button", { name: "open" }))
    expect(screen.getByRole("dialog")).toBeInTheDocument()
    expect(router.state.location.pathname + router.state.location.search).toBe("/cards?q=a")
    expect(router.state.location.state).toMatchObject({ dialog: "filters" })

    await act(async () => {
      await router.navigate(-1)
    })
    expect(onClosed).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(router.state.location.state).toBeNull()
  })

  it("closing from the UI pops the entry it pushed", async () => {
    const onClosed = vi.fn()
    const { router } = await renderInRouter(<Harness onClosed={onClosed} />, {
      initialEntries: ["/cards"],
    })
    await userEvent.click(screen.getByRole("button", { name: "open" }))
    await userEvent.click(screen.getByRole("button", { name: "close" }))
    expect(onClosed).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(router.state.location.state).toBeNull()
    expect(router.state.location.pathname).toBe("/cards")
  })
})
