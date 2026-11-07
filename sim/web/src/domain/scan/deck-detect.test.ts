import { describe, expect, it } from "vitest"

import {
  type Box,
  deckTiles,
  lyingSideways,
  mergeDeckBoxes,
  placeTileFinds,
  turnedQuarter,
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

  it("drops a box that swallows two neighbouring cards", () => {
    const row = [0, 80, 160, 240, 320].map((x) => cardAt(x, 0))
    const merged = mergeDeckBoxes([...row, cardAt(0, 0, 140, 0.95)])
    expect(merged).toHaveLength(5)
    expect(merged.every((box) => box.quad[1][0] - box.quad[0][0] === 63)).toBe(true)
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
