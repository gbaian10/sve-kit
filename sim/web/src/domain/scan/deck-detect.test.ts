import { describe, expect, it } from "vitest"

import {
  type Box,
  deckTiles,
  lyingSideways,
  mergeDeckBoxes,
  placeTileFinds,
  placeWholeFinds,
  turnedQuarter,
  wholeTile,
} from "./deck-detect"
import type { Quad } from "./tensor"

/** An upright card-shaped box (63 × 88 proportions) with its top left at (x, y). */
const cardAt = (x: number, y: number, width = 63, score = 0.9): Box => {
  const height = (width * 88) / 63
  const quad: Quad = [
    [x, y],
    [x + width, y],
    [x + width, y + height],
    [x, y + height],
  ]
  return { score, quad }
}

describe("deck card finding", () => {
  it("covers the whole photo with half-overlapping tiles at every scale", () => {
    const tiles = deckTiles(1000, 600)
    for (const tile of tiles) {
      expect(tile.x + tile.side).toBeLessThanOrEqual(1000)
      expect(tile.y + tile.side).toBeLessThanOrEqual(600)
    }
    const full = tiles.filter((tile) => tile.side === 600)
    expect(full.map((tile) => tile.x)).toEqual([0, 300, 400])
    expect(tiles.some((tile) => tile.side === Math.floor(600 * 0.22))).toBe(true)
  })

  it("maps tile finds into the photo and drops ones cut by the tile edge or not card-shaped", () => {
    const tile = { x: 100, y: 50, side: 512 }
    const kept = cardAt(20, 20, 63)
    const cut = cardAt(1, 30, 63)
    const square: Box = {
      score: 0.9,
      quad: [
        [100, 100],
        [180, 100],
        [180, 180],
        [100, 180],
      ],
    }
    const placed = placeTileFinds([kept, cut, square], tile, 256)
    expect(placed).toHaveLength(1)
    expect(placed[0]?.quad[0]).toEqual([140, 90])
  })

  it("merges the same card found at several scales and reads cards row by row", () => {
    const boxes = [
      cardAt(200, 10),
      cardAt(10, 10),
      cardAt(12, 11, 61, 0.8),
      cardAt(105, 150),
      cardAt(5, 160),
    ]
    const merged = mergeDeckBoxes(boxes)
    expect(merged.map((box) => box.quad[0])).toEqual([
      [10, 10],
      [200, 10],
      [5, 160],
      [105, 150],
    ])
  })

  it("drops a weaker box that swallows two neighbouring cards", () => {
    const row = [0, 80, 160, 240, 320].map((x) => cardAt(x, 0))
    const merged = mergeDeckBoxes([...row, cardAt(0, 0, 140, 0.5)])
    expect(merged).toHaveLength(5)
    expect(merged.every((box) => box.quad[1][0] - box.quad[0][0] === 63)).toBe(true)
  })

  it("keeps the surer box when a small find sits inside a card", () => {
    const row = [0, 80, 160, 240, 320].map((x) => cardAt(x, 0))
    // The art of the second card found as a card of its own.
    const art = cardAt(95, 20, 30, 0.5)
    const merged = mergeDeckBoxes([...row, art])
    expect(merged).toHaveLength(5)
    expect(merged.map((box) => box.quad[0][0])).toEqual([0, 80, 160, 240, 320])
  })

  it("keeps a big card among small printed marks, judging sizes by the sure finds", () => {
    // A near card on a mat: number circles and print around it are found weakly and small.
    const marks = [0, 40, 80, 120, 160, 200].map((x) => cardAt(x, 400, 20, 0.45))
    const far = [0, 80, 160].map((x) => cardAt(x, 0))
    const near = cardAt(300, 300, 200)
    const merged = mergeDeckBoxes([...far, ...marks, near])
    expect(merged).toContainEqual(near)
    // Far smaller than the sure cards near them: not cards.
    expect(merged.filter((box) => box.score < 0.5)).toEqual([])
  })

  it("keeps a big card on its own but drops a big box holding two cards", () => {
    const row = [0, 80, 160, 240].map((x) => cardAt(x, 0))
    const alone = cardAt(500, 0, 200)
    // Holds the centres of two cards without covering half of either.
    const merged: Box = {
      score: 0.9,
      quad: [
        [25, 30],
        [120, 30],
        [120, 200],
        [25, 200],
      ],
    }
    const kept = mergeDeckBoxes([...row, alone, merged])
    expect(kept).toContainEqual(alone)
    expect(kept).not.toContainEqual(merged)
  })

  it("keeps the only sure card when nothing else sets the size", () => {
    const sure = cardAt(80, 80)
    // Weak finds whose centres fall inside it, each covering only a corner of it.
    const weak = [
      cardAt(49.5, 37, 63, 0.5),
      cardAt(110.5, 123, 63, 0.5),
      cardAt(300, 80, 63, 0.5),
      cardAt(450, 80, 63, 0.5),
    ]
    expect(mergeDeckBoxes([sure, ...weak])).toContainEqual(sure)
  })

  it("keeps the card when a find inside it is just as sure", () => {
    const row = [0, 80, 160, 240, 320].map((x) => cardAt(x, 0))
    const art = cardAt(95, 20, 30, 0.9)
    expect(mergeDeckBoxes([...row, art]).map((box) => box.quad[0][0])).toEqual([
      0, 80, 160, 240, 320,
    ])
  })

  it("adds whole-photo finds only for cards too big for the tiles", () => {
    expect(wholeTile(800, 600)).toEqual({ x: 0, y: -100, side: 800 })
    // In the 256-pixel finder input: one card 100 px tall (about 312 photo px), one 40 px.
    const big = cardAt(20, 20, 72)
    const small = cardAt(150, 150, 29)
    const placed = placeWholeFinds([big, small], 800, 600, 256)
    expect(placed).toHaveLength(1)
    expect(placed[0]?.quad[0]).toEqual([62.5, -37.5])
  })

  it("tells a card lying on its side and turns its corners a quarter", () => {
    const upright = cardAt(0, 0).quad
    expect(lyingSideways(upright)).toBe(false)
    const lying: Quad = [
      [0, 0],
      [88, 0],
      [88, 63],
      [0, 63],
    ]
    expect(lyingSideways(lying)).toBe(true)
    expect(turnedQuarter(lying)).toEqual([
      [0, 63],
      [0, 0],
      [88, 0],
      [88, 63],
    ])
  })
})
