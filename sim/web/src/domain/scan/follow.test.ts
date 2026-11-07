import { describe, expect, it } from "vitest"

import { QuadFollower } from "./follow"
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

describe("QuadFollower", () => {
  it("takes the best card when it has none yet", () => {
    const follower = new QuadFollower()
    expect(
      follower.follow([
        { quad: NEAR, key: 0.5 },
        { quad: FAR, key: 0.6 },
      ]),
    ).toEqual({ shown: FAR, found: FAR })
  })

  it("keeps the followed card when another is only a little better", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: NEAR, key: 0.6 }])
    const { found } = follower.follow([
      { quad: NEAR, key: 0.5 },
      { quad: FAR, key: 0.6 },
    ])
    expect(found).toEqual(NEAR)
  })

  it("moves to another card that is clearly better", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: NEAR, key: 0.6 }])
    const { shown } = follower.follow([
      { quad: NEAR, key: 0.2 },
      { quad: FAR, key: 0.8 },
    ])
    expect(shown).toEqual(FAR)
  })

  it("stays on one of two stacked cards that both overlap it", () => {
    const follower = new QuadFollower()
    const top = box(100, 100)
    const under = box(112, 104)
    follower.follow([{ quad: top, key: 0.7 }])
    for (const [a, b] of [
      [0.69, 0.7],
      [0.7, 0.69],
      [0.69, 0.7],
    ] as const) {
      const { found, shown } = follower.follow([
        { quad: top, key: a },
        { quad: under, key: b },
      ])
      expect(found).toEqual(top)
      expect(shown).toEqual(top)
    }
  })

  it("draws the corners of the same card half way, but reads this frame's corners", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: box(100, 100), key: 0.6 }])
    expect(follower.follow([{ quad: box(110, 100), key: 0.6 }])).toEqual({
      shown: box(105, 100),
      found: box(110, 100),
    })
  })

  it("lines up corners that start from another corner", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: box(100, 100), key: 0.6 }])
    const [a, b, c, d] = box(110, 100)
    expect(follower.follow([{ quad: [c, d, a, b], key: 0.6 }]).shown).toEqual(box(105, 100))
  })

  it("keeps drawing the last box through a short miss, without reading it", () => {
    const follower = new QuadFollower()
    follower.follow([{ quad: NEAR, key: 0.6 }])
    expect(follower.follow([])).toEqual({ shown: NEAR, found: null })
    expect(follower.follow([])).toEqual({ shown: NEAR, found: null })
    expect(follower.follow([])).toEqual({ shown: null, found: null })
    // Starting over: the best card wins again.
    expect(
      follower.follow([
        { quad: NEAR, key: 0.5 },
        { quad: FAR, key: 0.6 },
      ]).found,
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
      ]).found,
    ).toEqual(FAR)
  })
})
