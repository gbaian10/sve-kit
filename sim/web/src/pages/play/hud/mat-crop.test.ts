import { describe, expect, it } from "vitest"

import {
  cropRect,
  frameFrom,
  framingOf,
  keptSize,
  MAX_ZOOM,
  moveFrame,
  placePicture,
} from "./mat-crop"

const WIDE = { w: 4000, h: 2000 }
const TALL = { w: 1000, h: 3000 }
const CENTRE = { x: 0.5, y: 0.5 }

describe("cropRect", () => {
  it("is the largest frame of the mat's shape the picture holds", () => {
    // A 3:1 mat on a 2:1 picture: the whole width, the middle band of the height.
    expect(cropRect(WIDE, 3, 1, CENTRE)).toEqual({
      x: 0,
      y: (2000 - 4000 / 3) / 2,
      w: 4000,
      h: 4000 / 3,
    })
    // The same on a tall picture: the whole width again, a thin band.
    expect(cropRect(TALL, 2, 1, CENTRE)).toEqual({ x: 0, y: 1250, w: 1000, h: 500 })
    // A mat taller than the picture's shape: the whole height, the middle of the width.
    expect(cropRect(WIDE, 1, 1, CENTRE)).toEqual({ x: 1000, y: 0, w: 2000, h: 2000 })
  })

  it("shrinks with the zoom, is placed by the pan, and never leaves the picture", () => {
    expect(cropRect(WIDE, 2, 2, { x: 0, y: 0 })).toEqual({ x: 0, y: 0, w: 2000, h: 1000 })
    expect(cropRect(WIDE, 2, 2, { x: 1, y: 1 })).toEqual({ x: 2000, y: 1000, w: 2000, h: 1000 })
    expect(cropRect(WIDE, 2, 0.2, CENTRE)).toEqual(cropRect(WIDE, 2, 1, CENTRE))
    expect(cropRect(WIDE, 2, 99, CENTRE).w).toBe(4000 / MAX_ZOOM)
    expect(cropRect(WIDE, 2, 2, { x: -3, y: 7 })).toEqual(cropRect(WIDE, 2, 2, { x: 0, y: 1 }))
  })
})

describe("moveFrame", () => {
  it("lets the frame follow the pointer, by the scale the picture is shown at", () => {
    const rect = cropRect(WIDE, 2, 2, CENTRE)
    // 4000 picture pixels shown 800 wide: one screen pixel is five. 100 px right is 500 of the
    // 2000 free picture pixels: a quarter of the room.
    expect(moveFrame(WIDE, rect, CENTRE, 800, 100, 0)).toEqual({ x: 0.75, y: 0.5 })
    expect(moveFrame(WIDE, rect, CENTRE, 800, 0, -100)).toEqual({ x: 0.5, y: 0 })
  })

  it("stops at the picture's edge and stays put where there is no room", () => {
    const rect = cropRect(WIDE, 2, 2, CENTRE)
    expect(moveFrame(WIDE, rect, CENTRE, 800, -9000, 0)).toEqual({ x: 0, y: 0.5 })
    // At zoom 1 a 2:1 frame fills a 2:1 picture: nowhere to go.
    const whole = cropRect(WIDE, 2, 1, CENTRE)
    expect(moveFrame(WIDE, whole, CENTRE, 800, 300, 300)).toEqual(CENTRE)
  })
})

describe("a kept framing", () => {
  it("records the frame by shares of the picture, and finds it again", () => {
    const rect = cropRect(WIDE, 2, 2, { x: 0.25, y: 1 })
    const framing = framingOf(WIDE, rect, 2, "board")
    expect(framing).toEqual({ cx: 0.375, cy: 0.75, span: 0.5, aspect: 2, reach: "board" })
    expect(frameFrom(WIDE, framing, 2)).toEqual({ zoom: 2, pan: { x: 0.25, y: 1 } })
  })

  it("keeps its centre when the frame takes another shape", () => {
    const framing = framingOf(WIDE, cropRect(WIDE, 2, 2, { x: 0.25, y: 1 }), 2, "board")
    // A squarer frame of the same width no longer fits at that height: it is as close as it can be.
    const again = frameFrom(WIDE, framing, 1)
    const rect = cropRect(WIDE, 1, again.zoom, again.pan)
    expect(rect.w).toBe(2000)
    expect((rect.x + rect.w / 2) / WIDE.w).toBe(0.375)
    expect(rect.y + rect.h).toBeLessThanOrEqual(WIDE.h)
  })
})

describe("placePicture", () => {
  const BOARD = { left: 400, top: 100, width: 1000, height: 500 }
  const TABLE = { left: 300, top: 0, width: 1200, height: 800 }
  // The middle half of the picture, framed for the board.
  const framing = { cx: 0.5, cy: 0.5, span: 0.5, aspect: 2, reach: "board" } as const

  it("covers the framed reach's box with the frame, centre on centre", () => {
    // 2000 picture pixels across 1000 screen pixels: the whole picture is 2000 × 1000 on screen.
    expect(placePicture(WIDE, framing, BOARD, BOARD)).toEqual({
      left: -500,
      top: -250,
      width: 2000,
      height: 1000,
    })
  })

  it("leaves the picture where it is for a larger box, which shows more of it", () => {
    const board = placePicture(WIDE, framing, BOARD, BOARD)
    const table = placePicture(WIDE, framing, BOARD, TABLE)
    // The same size on screen: nothing was enlarged.
    expect([table.width, table.height]).toEqual([board.width, board.height])
    // And the same place on screen, each measured from its own box.
    expect(TABLE.left + table.left).toBe(BOARD.left + board.left)
    expect(TABLE.top + table.top).toBe(BOARD.top + board.top)
  })

  it("moves the picture, and enlarges it only as far as it must, where it ends before the box", () => {
    // The whole picture framed for the board: the table has nothing more to show.
    const whole = { cx: 0.5, cy: 0.5, span: 1, aspect: 2, reach: "board" } as const
    const table = placePicture(WIDE, whole, BOARD, TABLE)
    // Enlarged to the table's height, no further.
    expect([table.width, table.height]).toEqual([1600, 800])
    expect(table.top).toBe(0)
    expect(table.left).toBeLessThanOrEqual(0)
    expect(table.left + table.width).toBeGreaterThanOrEqual(TABLE.width)
    // A frame at the picture's corner: the picture is pushed back until it covers the box.
    const corner = { cx: 0.25, cy: 0.25, span: 0.5, aspect: 2, reach: "board" } as const
    const pushed = placePicture(WIDE, corner, BOARD, TABLE)
    expect([pushed.left, pushed.top]).toEqual([0, 0])
    expect([pushed.width, pushed.height]).toEqual([2000, 1000])
  })
})

describe("keptSize", () => {
  it("keeps a picture as it is unless its long side is longer than any screen needs", () => {
    expect(keptSize(WIDE)).toBe(WIDE)
    expect(keptSize({ w: 8192, h: 4096 })).toEqual({ w: 4096, h: 2048 })
    expect(keptSize({ w: 100, h: 8192 })).toEqual({ w: 50, h: 4096 })
  })
})
