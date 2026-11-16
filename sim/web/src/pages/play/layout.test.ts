// @vitest-environment node
import { describe, expect, it } from "vitest"

import { computeLayout, sidebarFloats } from "./layout"

const single = (vw: number, vh: number, sidebar = true) =>
  computeLayout(vw, vh, { sidebar, dual: false })
/** A field card's width on screen. */
const cardWidth = (vw: number, vh: number) => Math.round(single(vw, vh).u * 100)

describe("computeLayout", () => {
  it("gives a desktop both side columns and field cards at least 72 px wide", () => {
    const layout = single(1912, 914)
    expect(layout).toMatchObject({
      plates: "right",
      hand: "row",
      sidebar: "open",
      sidebarOverlay: false,
      log: "column",
      stacked: false,
    })
    expect(cardWidth(1912, 914)).toBeGreaterThanOrEqual(72)
    // The board sits between the sidebar and the log, and above the hand.
    expect(layout.boardLeft).toBeGreaterThanOrEqual(layout.sidebarW)
    expect(layout.boardTop + layout.boardH * layout.u).toBeLessThanOrEqual(layout.handArea.top)
  })

  it("gives the table the room between the side panels, top to bottom", () => {
    // A desktop: from the card-info column to the log, the whole height.
    const wide = single(1912, 914)
    expect(wide.table).toEqual({
      left: wide.sidebarW,
      right: 1912 - wide.sideColW - wide.logW,
      top: 0,
      bottom: 914,
    })
    // An upright phone has no side panels: everything under the players' strip.
    const phone = single(390, 844)
    expect(phone.table).toEqual({ left: 0, right: 390, top: phone.topH, bottom: 844 })
    // The board and both hands lie inside it.
    expect(wide.boardLeft).toBeGreaterThanOrEqual(wide.table.left)
    expect(wide.boardLeft + wide.matW * wide.u).toBeLessThanOrEqual(wide.table.right)
    expect(wide.handArea.left).toBeGreaterThanOrEqual(wide.table.left)
  })

  it("keeps the board where it is when the sidebar of a wide screen is closed", () => {
    const open = single(1600, 900)
    const closed = single(1600, 900, false)
    expect(closed.sidebar).toBe("closed")
    expect([closed.boardLeft, closed.u]).toEqual([open.boardLeft, open.u])
  })

  it("floats the sidebar over the board on a tablet held sideways", () => {
    expect(sidebarFloats(1180, 820)).toBe(true)
    expect(sidebarFloats(1440, 900)).toBe(false)
    expect(sidebarFloats(820, 1180)).toBe(false)
    const open = single(1180, 820)
    expect(open).toMatchObject({ sidebar: "open", sidebarOverlay: true, log: "none" })
    expect(open.boardLeft).toBe(single(1180, 820, false).boardLeft)
  })

  it("puts the player plates on top when the screen is upright", () => {
    expect(single(820, 1180)).toMatchObject({
      portrait: true,
      plates: "top",
      narrow: true,
      stacked: false,
      sidebar: "none",
    })
  })

  it("stacks the piles on a phone held upright, which makes the cards wider", () => {
    const layout = single(390, 844)
    expect(layout).toMatchObject({ stacked: true, plates: "top", hand: "row" })
    // The official arrangement squeezed into this width leaves a card about 35 px wide.
    expect(cardWidth(390, 844)).toBeGreaterThanOrEqual(50)
    expect(layout.boardLeft).toBeGreaterThanOrEqual(0)
    expect(layout.boardLeft + layout.matW * layout.u).toBeLessThanOrEqual(390)
  })

  it("stands the hand beside the board on a phone held sideways", () => {
    const layout = single(844, 390)
    expect(layout).toMatchObject({ plates: "side", hand: "column", compact: true, stacked: false })
    expect(layout.boardLeft).toBeGreaterThanOrEqual(layout.handArea.right)
  })

  it("lays out one half of the dual view without the side column", () => {
    const half = computeLayout(956, 914, { sidebar: true, dual: true })
    expect(half).toMatchObject({ dual: true, sideColW: 0, sidebar: "none", portrait: false })
    expect(half.boardLeft + half.matW * half.u).toBeLessThanOrEqual(956)
  })
})
