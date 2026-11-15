// @vitest-environment node
import { describe, expect, it } from "vitest"

import { boardSize, matGeometry, type PileName, type Rect } from "./geometry"

const PILES: readonly PileName[] = ["leader", "deck", "cemetery", "evolve", "banish"]
const ROWS = ["field", "ex"] as const
const SLOTS = [0, 1, 2, 3, 4]

const overlap = (a: Rect, b: Rect): boolean =>
  a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h

describe.each([
  ["the official arrangement", false],
  ["the stacked arrangement", true],
] as const)("matGeometry: %s", (_name, stacked) => {
  const size = boardSize(stacked)
  const widths = [size.matWMin, size.matWMax]

  it.each(widths)("keeps every zone inside a mat %d units wide", (matW) => {
    const mat = matGeometry(matW, stacked)
    for (const opp of [false, true]) {
      const rects = [
        ...PILES.map((pile) => mat.pile(pile, opp)),
        ...ROWS.map((row) => mat.rowFrame(row, opp)),
        ...ROWS.flatMap((row) => SLOTS.map((slot) => mat.slotCard(row, slot, opp))),
        mat.points(opp),
        mat.pp(opp),
      ]
      for (const r of rects) {
        expect(r.x).toBeGreaterThanOrEqual(0)
        expect(r.y).toBeGreaterThanOrEqual(0)
        expect(r.x + r.w).toBeLessThanOrEqual(matW)
        expect(r.y + r.h).toBeLessThanOrEqual(mat.matH)
      }
    }
  })

  it.each(widths)("keeps piles, points, PP and slots apart at %d units", (matW) => {
    const mat = matGeometry(matW, stacked)
    const zones = [
      ...PILES.map((pile) => mat.pile(pile, false)),
      ...ROWS.flatMap((row) => SLOTS.map((slot) => mat.slotCard(row, slot, false))),
      mat.points(false),
      mat.pp(false),
    ]
    zones.forEach((a, i) => {
      zones.slice(i + 1).forEach((b) => {
        expect(overlap(a, b)).toBe(false)
      })
    })
  })

  it("turns the opponent's mat half a turn: slot 0 ends up on the other side", () => {
    const mat = matGeometry(size.matWMin, stacked)
    const own = mat.slotCard("field", 0, false)
    const theirs = mat.slotCard("field", 0, true)
    expect(theirs.x + theirs.w).toBeCloseTo(mat.matW - own.x)
    expect(theirs.y + theirs.h).toBeCloseTo(mat.matH - own.y)
  })

  it("puts the opponent's mat above the viewer's on the board", () => {
    const mat = matGeometry(size.matWMin, stacked)
    const slot = mat.slotCard("field", 0, false)
    expect(mat.onBoard(slot, true).y).toBe(slot.y)
    expect(mat.onBoard(slot, false).y).toBeGreaterThan(mat.matH)
    expect(mat.matH * 2).toBeLessThan(size.boardH)
  })
})

describe("the stacked arrangement", () => {
  it("draws its piles smaller than the cards in play", () => {
    const mat = matGeometry(boardSize(true).matWMin, true)
    expect(mat.pile("deck", false).w).toBeLessThan(mat.slotCard("field", 0, false).w)
    expect(mat.stacked).toBe(true)
  })
})
