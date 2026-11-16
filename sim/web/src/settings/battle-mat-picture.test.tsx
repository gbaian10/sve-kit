import "fake-indexeddb/auto"

import { act, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import {
  clearBattleMatPicture,
  saveBattleMatPicture,
  useBattleMatPicture,
} from "./battle-mat-picture"

const SIZE = { w: 800, h: 400 }
const FRAMING = { cx: 0.5, cy: 0.5, span: 1, aspect: 2, reach: "board" } as const

function Shown() {
  const kept = useBattleMatPicture()
  return <p>{kept === null ? "none" : `${kept.url} ${String(kept.framing.span)}`}</p>
}

afterEach(async () => {
  await act(() => clearBattleMatPicture())
  vi.restoreAllMocks()
})

describe("the player's own mat picture", () => {
  it("has none until one is kept, shows the kept one, and none again once it is removed", async () => {
    let made = 0
    const create = vi
      .spyOn(URL, "createObjectURL")
      .mockImplementation(() => `blob:mat-${String(++made)}`)
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined)
    render(<Shown />)
    expect(screen.getByText("none")).toBeInTheDocument()
    const first = new Blob(["picture"], { type: "image/webp" })
    let kept = false
    await act(async () => {
      kept = await saveBattleMatPicture(first, SIZE, FRAMING)
    })
    expect(kept).toBe(true)
    expect(await screen.findByText("blob:mat-1 1")).toBeInTheDocument()
    // The same picture framed anew keeps its address: nothing has to load again.
    await act(async () => {
      await saveBattleMatPicture(first, SIZE, { ...FRAMING, span: 0.5 })
    })
    expect(await screen.findByText("blob:mat-1 0.5")).toBeInTheDocument()
    expect(revoke).not.toHaveBeenCalled()
    // Another picture replaces it, and the first one's address is given back.
    await act(async () => {
      await saveBattleMatPicture(new Blob(["another"], { type: "image/webp" }), SIZE, FRAMING)
    })
    expect(await screen.findByText("blob:mat-2 1")).toBeInTheDocument()
    expect(revoke).toHaveBeenCalledWith("blob:mat-1")
    await act(() => clearBattleMatPicture())
    await waitFor(() => {
      expect(screen.getByText("none")).toBeInTheDocument()
    })
    expect(create).toHaveBeenCalledTimes(2)
  })

  it("reports a picture the browser would not store, and shows none", async () => {
    vi.spyOn(IDBDatabase.prototype, "transaction").mockImplementation(() => {
      throw new Error("storage is blocked")
    })
    render(<Shown />)
    let kept = true
    await act(async () => {
      kept = await saveBattleMatPicture(new Blob(["picture"]), SIZE, FRAMING)
    })
    expect(kept).toBe(false)
    expect(screen.getByText("none")).toBeInTheDocument()
  })
})
