import { fireEvent, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { I18nextProvider } from "react-i18next"
import { beforeAll, describe, expect, it, vi } from "vitest"

import { createI18n } from "../../i18n"
import { Dialog } from "./Dialog"

let i18n: Awaited<ReturnType<typeof createI18n>>
beforeAll(async () => {
  i18n = await createI18n("zh-TW")
})

function Harness({ onClose = () => undefined }: { readonly onClose?: () => void }) {
  const [open, setOpen] = useState(false)
  return (
    <I18nextProvider i18n={i18n}>
      <button
        type="button"
        onClick={() => {
          setOpen(true)
        }}
      >
        open
      </button>
      <Dialog
        open={open}
        onClose={() => {
          onClose()
          setOpen(false)
        }}
        title="篩選"
      >
        <p>body</p>
      </Dialog>
    </I18nextProvider>
  )
}

describe("Dialog", () => {
  it("opens as a modal with focus on the close button, then returns focus to the opener", async () => {
    render(<Harness />)
    const opener = screen.getByRole("button", { name: "open" })
    await userEvent.click(opener)
    const dialog = screen.getByRole("dialog")
    expect(dialog).toHaveAttribute("open")
    expect(dialog).toHaveAccessibleName("篩選")
    const close = screen.getByRole("button", { name: "關閉" })
    expect(close).toHaveFocus()
    await userEvent.click(close)
    expect(dialog).not.toHaveAttribute("open")
    expect(opener).toHaveFocus()
  })

  it("routes Escape (the native cancel event) through onClose", async () => {
    const onClose = vi.fn()
    render(<Harness onClose={onClose} />)
    await userEvent.click(screen.getByRole("button", { name: "open" }))
    const dialog = screen.getByRole("dialog")
    fireEvent(dialog, new Event("cancel", { cancelable: true }))
    expect(onClose).toHaveBeenCalledTimes(1)
    // A closed <dialog> leaves the accessibility tree, so assert on the element itself.
    expect(dialog).not.toHaveAttribute("open")
  })

  it("closes on a backdrop click but not on a click inside the panel", async () => {
    const onClose = vi.fn()
    render(<Harness onClose={onClose} />)
    await userEvent.click(screen.getByRole("button", { name: "open" }))
    await userEvent.click(screen.getByText("body"))
    expect(onClose).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("dialog"))
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})
