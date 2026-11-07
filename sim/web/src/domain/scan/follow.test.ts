import { describe, expect, it } from "vitest"

import { boxOverlap, QuadFollower } from "./follow"
import type { Quad } from "./tensor"

/** An upright box, corners clockwise from the top left. */
function box(x: number, y: number, w = 60, h = 84): Quad {
  return [
    [x, y],
    [x + w, y],
    [x + w, y + h],
    [x, y + h],
  ]
}

const NEAR = box(100, 100)
const FAR = box(300, 100)

describe("boxOverlap", () => {
  it("is 1 for the same box and 0 for boxes apart", () => {
    expect(boxOverlap(NEAR, NEAR)).toBe(1)
    expect(boxOverlap(NEAR, FAR)).toBe(0)
  })

  it("is the shared area over the joined area", () => {
    // Half of each 60-wide box overlaps: 30 / (60 + 60 - 30).
    expect(boxOverlap(box(0, 0), box(30, 0))).toBeCloseTo(1 / 3)
  })
})

describe("QuadFollower", () => {
  it("takes the best card when it has none yet", () => {
    const follower = new QuadFollower()
    expect(
      follower.follow([
        { quad: NEAR, key: 0.5 },
        { quad: FAR, key: 0.6 },
      ]),
    ).toEqual(FAR)
  })

  it("keeps the followed card when another is only a little better", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: NEAR, key: 0.6 }])
    const shown = follower.follow([
      { quad: NEAR, key: 0.5 },
      { quad: FAR, key: 0.6 },
    ])
    expect(shown).toEqual(NEAR)
  })

  it("moves to another card that is clearly better", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: NEAR, key: 0.6 }])
    expect(
      follower.follow([
        { quad: NEAR, key: 0.2 },
        { quad: FAR, key: 0.8 },
      ]),
    ).toEqual(FAR)
  })

  it("moves the corners of the same card half way", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: box(100, 100), key: 0.6 }])
    expect(follower.follow([{ quad: box(110, 100), key: 0.6 }])).toEqual(box(105, 100))
  })

  it("lines up corners that start from another corner", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: box(100, 100), key: 0.6 }])
    const [a, b, c, d] = box(110, 100)
    expect(follower.follow([{ quad: [c, d, a, b], key: 0.6 }])).toEqual(box(105, 100))
  })

  it("keeps the last card through a short miss, then lets go", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: NEAR, key: 0.6 }])
    expect(follower.follow([])).toEqual(NEAR)
    expect(follower.follow([])).toEqual(NEAR)
    expect(follower.follow([])).toBeNull()
    // Starting over: the best card wins again.
    expect(
      follower.follow([
        { quad: NEAR, key: 0.5 },
        { quad: FAR, key: 0.6 },
      ]),
    ).toEqual(FAR)
  })

  it("forgets the card on reset", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: NEAR, key: 0.6 }])
    follower.reset()
    expect(
      follower.follow([
        { quad: NEAR, key: 0.5 },
        { quad: FAR, key: 0.6 },
      ]),
    ).toEqual(FAR)
  })
})
