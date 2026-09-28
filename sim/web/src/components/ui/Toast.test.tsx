import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
import { Button } from "./Button"
import { useToast } from "./toast-context"

function Harness({ onUndo }: { readonly onUndo: () => void }) {
  const toast = useToast()
  return (
    <Button
      onClick={() => {
        toast.show({ message: "已移除", action: { label: "復原", onClick: onUndo } })
      }}
    >
      remove
    </Button>
  )
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
})
afterEach(() => {
  vi.useRealTimers()
})

describe("Toast", () => {
  it("announces the message, runs the action and disappears", async () => {
    const onUndo = vi.fn()
    await renderInRouter(<Harness onUndo={onUndo} />)
    await userEvent.click(screen.getByRole("button", { name: "remove" }))
    expect(screen.getByRole("status")).toHaveTextContent("已移除")
    await userEvent.click(screen.getByRole("button", { name: "復原" }))
    expect(onUndo).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole("status")).not.toBeInTheDocument()
  })

  it("times out on its own", async () => {
    await renderInRouter(<Harness onUndo={() => undefined} />)
    await userEvent.click(screen.getByRole("button", { name: "remove" }))
    expect(screen.getByRole("status")).toBeInTheDocument()
    act(() => {
      vi.advanceTimersByTime(5100)
    })
    expect(screen.queryByRole("status")).not.toBeInTheDocument()
  })
})
