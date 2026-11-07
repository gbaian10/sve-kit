import { describe, expect, it } from "vitest"

import { decodeDetections, writeSample } from "./tensor"

describe("scan tensors", () => {
  it("turns card-centre peaks and corner offsets into quads in input pixels", () => {
    const grid = 4
    const cells = grid * grid
    const heat = new Float32Array(cells)
    const corners = new Float32Array(8 * cells)
    heat[1 * grid + 1] = 0.9 // a card centred in cell (1, 1)
    heat[1 * grid + 2] = 0.6 // its neighbour, not a peak
    heat[3 * grid + 3] = 0.7 // a second card
    heat[0] = 0.2 // below the threshold
    const offsets = [-1, -1.5, 1, -1.5, 1, 1.5, -1, 1.5]
    offsets.forEach((offset, channel) => {
      corners[channel * cells + 1 * grid + 1] = offset
    })
    const found = decodeDetections(heat, corners, grid, 8, 0.5)
    expect(found.map((d) => d.score)).toEqual([expect.closeTo(0.9), expect.closeTo(0.7)])
    expect(found[0]?.quad).toEqual([
      [4, 0],
      [20, 0],
      [20, 24],
      [4, 24],
    ])
    expect(found[1]?.quad[0]).toEqual([28, 28])
  })

  it("writes RGBA pixels as RGB planes of a batch slot, optionally turned 180°", () => {
    const rgba = Uint8ClampedArray.from([255, 0, 0, 255, 0, 255, 255, 255])
    const batch = new Float32Array(2 * 3 * 2)
    writeSample(rgba, batch, 0)
    writeSample(rgba, batch, 1, true)
    expect(Array.from(batch.subarray(0, 6))).toEqual([1, 0, 0, 1, 0, 1])
    expect(Array.from(batch.subarray(6, 12))).toEqual([0, 1, 1, 0, 1, 0])
  })
})
