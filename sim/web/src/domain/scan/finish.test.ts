import { describe, expect, it } from "vitest"

import {
  FINISH_MARGIN,
  finishGroups,
  finishKey,
  finishOf,
  finishScores,
  FinishSmoother,
  hueGap,
  pickFinish,
} from "./finish"
import { parseScanCards } from "./search"

const W = 120
const H = 168
type Rgb = readonly [number, number, number]

const GREEN: Rgb = [40, 150, 70]
const PINK: Rgb = [200, 80, 190]
const GEMS: readonly Rgb[] = [
  [230, 60, 60],
  [240, 200, 40],
]

/** A card: grey art, a name bar along the bottom fifth, and two corner gems. */
function card(
  bar: Rgb,
  gems: readonly Rgb[] = [GREEN, GREEN],
  light: Rgb = [1, 1, 1],
): Uint8ClampedArray {
  const pixels = new Uint8ClampedArray(W * H * 4)
  for (let y = 0; y < H; y += 1)
    for (let x = 0; x < W; x += 1) {
      const fx = x / W
      const fy = y / H
      let rgb: Rgb = [120, 120, 120]
      if (fy >= 0.82 && fy < 0.93) {
        if (fx < 0.16) rgb = gems[0] ?? GREEN
        else if (fx >= 0.84) rgb = gems[1] ?? GREEN
        else if (fx >= 0.2 && fx < 0.8 && fy < 0.88) rgb = bar
      }
      const i = 4 * (y * W + x)
      pixels[i] = rgb[0] * light[0]
      pixels[i + 1] = rgb[1] * light[1]
      pixels[i + 2] = rgb[2] * light[2]
      pixels[i + 3] = 255
    }
  return pixels
}

function turned(pixels: Uint8ClampedArray): Uint8ClampedArray {
  const out = new Uint8ClampedArray(pixels.length)
  const n = W * H
  for (let i = 0; i < n; i += 1) out.set(pixels.subarray(4 * i, 4 * i + 4), 4 * (n - 1 - i))
  return out
}

const plainImage = card(GREEN)
const slImage = card(PINK, GEMS)
const references = [finishOf(plainImage, W, H), finishOf(slImage, W, H)]
const photo = (pixels: Uint8ClampedArray) =>
  [finishOf(pixels, W, H), finishOf(pixels, W, H, true)] as const

const [plain, sl, other] = parseScanCards(
  [
    { no: "BP01-001", c: "c1", n: 1, f: 0, nm: "a", a: "x", k: "elf" },
    { no: "BP01-SL01", c: "c1", n: 2, f: 0, nm: "a", a: "x", k: "elf" },
    { no: "BP01-002", c: "c2", n: 3, f: 0, nm: "b", a: "y", k: "elf" },
  ],
  3,
)
if (!plain || !sl || !other) throw new Error("fixture cards")
const group = [plain, sl]

describe("printing frame check", () => {
  it("measures hue as the angle between chroma vectors", () => {
    expect(hueGap([1, 0], [0, 1])).toBeCloseTo(90)
    expect(hueGap([2, 2], [1, 1])).toBeCloseTo(0)
  })

  it("picks the SL printing for an SL card, even under a warm light", () => {
    const scores = finishScores(references, photo(card(PINK, GEMS, [1.2, 1, 0.8])))
    expect(scores).not.toBeNull()
    expect(pickFinish(group, scores ?? [], plain)).toBe(sl)
  })

  it("reads a card lying upside down", () => {
    const scores = finishScores(references, photo(turned(slImage)))
    expect(pickFinish(group, scores ?? [], plain)).toBe(sl)
  })

  it("keeps the plain printing for a plain card", () => {
    const scores = finishScores(references, photo(card(GREEN, [GREEN, GREEN], [0.9, 1, 1.1])))
    expect(pickFinish(group, scores ?? [], plain)).toBe(plain)
  })

  it("has nothing to compare when the printings share one frame (premium printings)", () => {
    const same = [finishOf(plainImage, W, H), finishOf(plainImage, W, H)]
    expect(finishScores(same, photo(slImage))).toBeNull()
  })

  it("keeps the plain printing unless another one leads by the margin", () => {
    expect(pickFinish(group, [20, 20 - FINISH_MARGIN + 0.1], plain)).toBe(plain)
    expect(pickFinish(group, [20, 20 - FINISH_MARGIN], plain)).toBe(sl)
    expect(pickFinish(group, [20, 5], other)).toBe(other)
  })

  it("groups the printings of one card, art and face only", () => {
    const groups = finishGroups([plain, sl, other])
    expect([...groups.keys()]).toEqual([finishKey(plain)])
    expect(groups.get(finishKey(plain))).toEqual([plain, sl])
  })

  it("does not let one odd frame flip the live answer", () => {
    const smoother = new FinishSmoother()
    expect(pickFinish(group, smoother.update("k", [5, 40]), plain)).toBe(plain)
    expect(pickFinish(group, smoother.update("k", [40, 5]), plain)).toBe(plain)
    smoother.update("k", [40, 5])
    smoother.update("k", [40, 5])
    expect(pickFinish(group, smoother.update("k", [40, 5]), plain)).toBe(sl)
    expect(pickFinish(group, smoother.update("other", [5, 40]), plain)).toBe(plain)
  })

  it("starts over after another card led, even one that could not be scored", () => {
    const smoother = new FinishSmoother()
    for (let i = 0; i < 6; i += 1) smoother.update("a", [40, 5])
    smoother.follow("b") // a card with a single printing: nothing to score
    expect(pickFinish(group, smoother.update("a", [5, 40]), plain)).toBe(plain)
  })
})
