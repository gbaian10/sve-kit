import { describe, expect, it } from "vitest"

import { orderQuad, warpQuad } from "./geometry"
import type { Point } from "./tensor"

/** A 4×4 image whose four 2×2 quadrants are red, green, blue and white. */
function quadrants() {
  const colours = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 255],
  ]
  const data = new Uint8ClampedArray(4 * 4 * 4)
  for (let y = 0; y < 4; y += 1)
    for (let x = 0; x < 4; x += 1) {
      const colour = colours[(y < 2 ? 0 : 2) + (x < 2 ? 0 : 1)] ?? [0, 0, 0]
      data.set([...colour, 255], (y * 4 + x) * 4)
    }
  return { width: 4, height: 4, data }
}

const pixel = (data: Uint8ClampedArray, at: number) => Array.from(data.subarray(at * 4, at * 4 + 3))

describe("scan geometry", () => {
  it("orders corners as seen, and turns them for a card lying on its side", () => {
    const shuffled: Point[] = [
      [90, 110],
      [10, 12],
      [8, 100],
      [95, 5],
    ]
    expect(orderQuad(shuffled)).toEqual([
      [10, 12],
      [95, 5],
      [90, 110],
      [8, 100],
    ])
    expect(orderQuad(shuffled, true)).toEqual([
      [8, 100],
      [10, 12],
      [95, 5],
      [90, 110],
    ])
  })

  it("straightens a quad into the requested size, upright or turned", () => {
    const image = quadrants()
    const upright = warpQuad(
      image,
      [
        [0, 0],
        [4, 0],
        [4, 4],
        [0, 4],
      ],
      2,
      2,
    )
    expect([0, 1, 2, 3].map((at) => pixel(upright, at))).toEqual([
      [255, 0, 0],
      [0, 255, 0],
      [0, 0, 255],
      [255, 255, 255],
    ])
    const turned = warpQuad(
      image,
      [
        [4, 4],
        [0, 4],
        [0, 0],
        [4, 0],
      ],
      2,
      2,
    )
    expect(pixel(turned, 0)).toEqual([255, 255, 255])
    expect(pixel(turned, 3)).toEqual([255, 0, 0])
    expect(turned[3]).toBe(255)
  })

  it("follows a perspective (keystoned) quad to its corners", () => {
    const image = quadrants()
    const keystone = warpQuad(
      image,
      [
        [0.5, 0],
        [3.5, 0],
        [4, 4],
        [0, 4],
      ],
      2,
      2,
    )
    expect([0, 1, 2, 3].map((at) => pixel(keystone, at))).toEqual([
      [255, 0, 0],
      [0, 255, 0],
      [0, 0, 255],
      [255, 255, 255],
    ])
  })
})
