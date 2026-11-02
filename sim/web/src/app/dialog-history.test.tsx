import { act, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { type ReactNode, useState } from "react"
import { describe, expect, it, vi } from "vitest"

import { renderInRouter } from "../test-utils"
import { useDialogHistory, useDropDialogs } from "./dialog-history"

function Harness({
  onClosed,
  id = "filters",
  children,
}: {
  readonly onClosed: () => void
  readonly id?: string
  readonly children?: ReactNode
}) {
  const [open, setOpen] = useState(false)
  const close = useDialogHistory(id, open, () => {
    onClosed()
    setOpen(false)
  })
  const dropAll = useDropDialogs()
  return (
    <div>
      <button
        type="button"
        onClick={() => {
          setOpen(true)
        }}
      >
        {`open ${id}`}
      </button>
      <button type="button" onClick={dropAll}>
        {`drop from ${id}`}
      </button>
      {open && (
        <div role="dialog" aria-label={id}>
          {children}
          <button type="button" onClick={close}>
            {`close ${id}`}
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
    await userEvent.click(screen.getByRole("button", { name: "open filters" }))
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
    await userEvent.click(screen.getByRole("button", { name: "open filters" }))
    await userEvent.click(screen.getByRole("button", { name: "close filters" }))
    expect(onClosed).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(router.state.location.state).toBeNull()
    expect(router.state.location.pathname).toBe("/cards")
  })

  it("keeps an outer dialog open while one inside it opens and closes", async () => {
    const outerClosed = vi.fn()
    const innerClosed = vi.fn()
    const { router } = await renderInRouter(
      <Harness id="deck" onClosed={outerClosed}>
        <Harness id="export" onClosed={innerClosed} />
      </Harness>,
      { initialEntries: ["/decks/1"] },
    )
    await userEvent.click(screen.getByRole("button", { name: "open deck" }))
    await userEvent.click(screen.getByRole("button", { name: "open export" }))
    expect(screen.getByRole("dialog", { name: "deck" })).toBeInTheDocument()
    expect(screen.getByRole("dialog", { name: "export" })).toBeInTheDocument()
    await act(async () => {
      await router.navigate(-1)
    })
    expect(innerClosed).toHaveBeenCalledTimes(1)
    expect(outerClosed).not.toHaveBeenCalled()
    expect(screen.getByRole("dialog", { name: "deck" })).toBeInTheDocument()
    await act(async () => {
      await router.navigate(-1)
    })
    expect(outerClosed).toHaveBeenCalledTimes(1)
    expect(router.state.location.state).toBeNull()
  })

  it("drops every open dialog with its history entry", async () => {
    const outerClosed = vi.fn()
    const innerClosed = vi.fn()
    const { router } = await renderInRouter(
      <Harness id="deck" onClosed={outerClosed}>
        <Harness id="export" onClosed={innerClosed} />
      </Harness>,
      { initialEntries: ["/decks/1"] },
    )
    // Nothing is open: dropping does not leave the page.
    await userEvent.click(screen.getByRole("button", { name: "drop from deck" }))
    expect(router.state.location.pathname).toBe("/decks/1")
    await userEvent.click(screen.getByRole("button", { name: "open deck" }))
    await userEvent.click(screen.getByRole("button", { name: "open export" }))
    await act(async () => {
      screen.getByRole("button", { name: "drop from export" }).click()
      await Promise.resolve()
    })
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    expect(outerClosed).toHaveBeenCalledTimes(1)
    expect(router.state.location.state).toBeNull()
    expect(router.state.location.pathname).toBe("/decks/1")
  })

  it("also drops entries pushed over a dialog that keep its state, such as a card opened from it", async () => {
    const onClosed = vi.fn()
    const { router } = await renderInRouter(<Harness id="deck" onClosed={onClosed} />, {
      initialEntries: ["/decks/1"],
    })
    await userEvent.click(screen.getByRole("button", { name: "open deck" }))
    await act(async () => {
      await router.navigate("/decks/1?card=c1", { state: router.state.location.state })
    })
    expect(router.state.location.state).toMatchObject({ dialogs: ["deck"] })
    await userEvent.click(screen.getByRole("button", { name: "drop from deck" }))
    await waitFor(() => {
      expect(router.state.location.state).toBeNull()
    })
    expect(router.state.location.pathname + router.state.location.search).toBe("/decks/1")
    expect(onClosed).toHaveBeenCalledTimes(1)
  })
})
