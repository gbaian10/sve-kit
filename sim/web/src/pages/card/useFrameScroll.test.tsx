import { fireEvent, render } from "@testing-library/react"
import { useRef } from "react"
import { beforeEach, describe, expect, it } from "vitest"

import { useFrameScroll } from "./useFrameScroll"

interface Shown {
  readonly entryKey: string
  readonly cardId: string | undefined
  readonly pending: boolean
}

function Frame({ entryKey, cardId, pending, modal = false }: Shown & { readonly modal?: boolean }) {
  const article = useRef<HTMLElement>(null)
  useFrameScroll({ article, active: true, pending, cardId, entryKey })
  // The modal's dialog body scrolls; it has no marker, the card's own wrapper sits inside it.
  return modal ? (
    <div data-testid="scroller">
      <div data-card-overlay="">
        <article ref={article} />
      </div>
    </div>
  ) : (
    <div data-card-scroll="" data-testid="scroller">
      <article ref={article} />
    </div>
  )
}

/** Reads the card at the scroller's current position, as the reader would have left it there. */
function setup(first: Shown, modal = false) {
  const view = render(<Frame {...first} modal={modal} />)
  const scroller = view.getByTestId("scroller")
  return {
    scroller,
    show: (next: Shown) => {
      view.rerender(<Frame {...next} modal={modal} />)
    },
    scrollTo: (top: number) => {
      scroller.scrollTop = top
      fireEvent.scroll(scroller)
    },
  }
}

// History keys are unique per entry; the saved positions outlive a render, so each case gets its own.
let run = 0
beforeEach(() => {
  run += 1
})

const card = (entryKey: string, cardId: string, pending = false): Shown => ({
  entryKey: `${String(run)}:${entryKey}`,
  cardId,
  pending,
})

describe("scrolling inside the card frame", () => {
  it("opens another card at the top when its data was already loaded", () => {
    const frame = setup(card("a", "c:a"))
    frame.scrollTo(300)
    frame.show(card("b", "c:b"))
    expect(frame.scroller.scrollTop).toBe(0)
  })

  it("opens another card at the top when it had to be downloaded first", () => {
    const frame = setup(card("a", "c:a"))
    frame.scrollTo(300)
    // The old card stays on screen while the new one loads: no position of the new entry yet.
    frame.show({ ...card("b", "c:a"), pending: true })
    expect(frame.scroller.scrollTop).toBe(300)
    frame.show(card("b", "c:b"))
    expect(frame.scroller.scrollTop).toBe(0)
  })

  it.each([false, true])(
    "restores the position left on an entry when going back to it (downloaded again: %s)",
    (reload) => {
      const frame = setup(card("a", "c:a"))
      frame.scrollTo(300)
      frame.show(card("b", "c:b"))
      frame.scrollTo(40)
      if (reload) frame.show({ ...card("a", "c:b"), pending: true })
      // A pending card still shows the other one: scrolling it must not overwrite the entry's own.
      if (reload) frame.scrollTo(10)
      frame.show(card("a", "c:a"))
      expect(frame.scroller.scrollTop).toBe(300)
      frame.show(card("b", "c:b"))
      expect(frame.scroller.scrollTop).toBe(40)
    },
  )

  it("keeps the position when another printing of the same card replaces the entry", () => {
    const frame = setup(card("a", "c:a"))
    frame.scrollTo(300)
    frame.show(card("b", "c:a"))
    expect(frame.scroller.scrollTop).toBe(300)
  })

  it("finds the scrolling body of the modal, which wraps the card without a marker", () => {
    const frame = setup(card("a", "c:a"), true)
    frame.scrollTo(300)
    frame.show(card("b", "c:b"))
    expect(frame.scroller.scrollTop).toBe(0)
    frame.show(card("a", "c:a"))
    expect(frame.scroller.scrollTop).toBe(300)
  })
})
