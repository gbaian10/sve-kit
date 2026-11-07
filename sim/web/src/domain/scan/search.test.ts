import { describe, expect, it } from "vitest"

import { classMask, isConfident, matchCards, parseScanCards, type ScanIndex } from "./search"

const SCORING = { hubWeight: 0.75, accept: 0.5, margin: 0.03 }

const rows = [
  { no: "BP01-001SL", c: "c1", n: 1, nm: "a", a: "x", k: "elf", f: 0 },
  { no: "BP01-001", c: "c1", n: 2, nm: "a", a: "x", k: "elf", f: 0 },
  { no: "BP01-002", c: "c2", n: 3, nm: "b", a: "x", k: "dragon", f: 0 },
  { no: "BP01-003", c: "c3", n: 4, nm: "c", a: "x", k: "neutral", f: 0 },
  { no: "BP01-T01", c: "c4", n: 5, nm: "d", a: "x", k: "", f: 0 },
]

function index(vectors: number[][], hub: number[] = vectors.map(() => 0)): ScanIndex {
  return {
    dim: 4,
    vectors: Int8Array.from(vectors.flat()),
    scale: Float32Array.from(vectors.map(() => 1 / 127)),
    hub: Float32Array.from(hub),
    cards: parseScanCards(rows.slice(0, vectors.length), vectors.length),
    scoring: SCORING,
  }
}

const unit = (...values: number[]) => Float32Array.from(values)

describe("scan card search", () => {
  it("ranks cards by similarity and shows the plain-numbered printing of a matched art", () => {
    const found = matchCards(
      index([
        [127, 0, 0, 0],
        [127, 0, 0, 0],
        [0, 127, 0, 0],
      ]),
      [unit(0.8, 0.6, 0, 0)],
      null,
      3,
    )
    expect(found.map((match) => match.card.no)).toEqual(["BP01-001", "BP01-002"])
    expect(found[0]?.raw).toBeCloseTo(0.8)
    expect(found[1]?.raw).toBeCloseTo(0.6)
  })

  it("keeps the runner-up card however many printings the best card has", () => {
    const many = {
      ...index([]),
      dim: 1,
      vectors: Int8Array.from([127, 127, 127, 127, 127, 127, 127, 127, 126]),
      scale: Float32Array.from(Array.from({ length: 9 }, () => 1 / 127)),
      hub: new Float32Array(9),
      cards: parseScanCards(
        [
          ...Array.from({ length: 8 }, (_, at) => ({ ...rows[0], no: `BP01-001P${String(at)}` })),
          rows[2],
        ],
        9,
      ),
    }
    const found = matchCards(many, [Float32Array.from([1])], null, 2)
    expect(found.map((match) => match.card.cardId)).toEqual(["c1", "c2"])
    expect(isConfident(found, SCORING)).toBe(false)
  })

  it("keeps a card's back as the back, even without an art group", () => {
    const faces = {
      ...index([]),
      dim: 1,
      vectors: Int8Array.from([-127, 127]),
      scale: Float32Array.from([1 / 127, 1 / 127]),
      hub: new Float32Array(2),
      cards: parseScanCards(
        [
          { no: "BP01-030", c: "c9", n: 9, f: 0, nm: "front", a: "", k: "" },
          { no: "BP01-030", c: "c9", n: 9, f: 1, nm: "back", a: "", k: "" },
        ],
        2,
      ),
    }
    const [best] = matchCards(faces, [Float32Array.from([1])], null, 1)
    expect(best?.card).toMatchObject({ face: 1, name: "back" })
    expect(best?.raw).toBeCloseTo(1)
  })

  it("scores a card by its best view, so a card turned 180° still matches", () => {
    const search = index([
      [127, 0, 0, 0],
      [127, 0, 0, 0],
      [0, 127, 0, 0],
    ])
    const upright = unit(0, 0.2, 0, 0)
    const turned = unit(1, 0, 0, 0)
    expect(matchCards(search, [upright, turned], null, 1)[0]?.card.cardId).toBe("c1")
  })

  it("subtracts the hub correction from the ranking but not from the raw similarity", () => {
    const search = index(
      [
        [127, 0, 0, 0],
        [127, 0, 0, 0],
        [0, 127, 0, 0],
      ],
      [0.4, 0.4, 0],
    )
    const [first, second] = matchCards(search, [unit(0.7, 0.7, 0, 0)], null, 2)
    expect(first?.card.cardId).toBe("c2")
    expect(second?.raw).toBeCloseTo(0.7)
    expect(second?.score).toBeCloseTo(0.7 - 0.75 * 0.4)
  })

  it("keeps only the picked classes, plus cards without a class", () => {
    const search = index([
      [127, 0, 0, 0],
      [127, 0, 0, 0],
      [0, 127, 0, 0],
      [0, 0, 127, 0],
      [0, 0, 0, 127],
    ])
    expect(classMask(search.cards, [])).toBeNull()
    const mask = classMask(search.cards, ["dragon"])
    const found = matchCards(search, [unit(0.5, 0.5, 0.5, 0.5)], mask, 5)
    expect(found.map((match) => match.card.cardId).sort()).toEqual(["c2", "c4"])
  })

  it("is confident only when the best card is close enough and ahead of the next one", () => {
    const [first, , second] = parseScanCards(rows, rows.length)
    if (!first || !second) throw new Error("card list fixture")
    const match = (raw: number, score = raw, at = 0) => ({ card: at ? second : first, raw, score })
    expect(isConfident([], SCORING)).toBe(false)
    expect(isConfident([match(0.9)], SCORING)).toBe(true)
    expect(isConfident([match(0.4)], SCORING)).toBe(false)
    expect(isConfident([match(0.9), match(0.89, 0.89, 2)], SCORING)).toBe(false)
    expect(isConfident([match(0.9), match(0.8, 0.8, 2)], SCORING)).toBe(true)
  })

  it("rejects a card list that does not match the index", () => {
    expect(() => parseScanCards(rows, 4)).toThrow()
    expect(() => parseScanCards([{ ...rows[0], n: "1" }], 1)).toThrow()
    expect(() => parseScanCards([{ ...rows[0], k: null }], 1)).toThrow()
    expect(() => parseScanCards([null], 1)).toThrow()
    expect(() => parseScanCards([{ ...rows[0], f: -1 }], 1)).toThrow()
  })

  it("ranks tied cards by their first row left after the class mask", () => {
    const tied: ScanIndex = {
      dim: 4,
      vectors: Int8Array.from([127, 0, 0, 0, 127, 0, 0, 0, 127, 0, 0, 0]),
      scale: Float32Array.from([1 / 127, 1 / 127, 1 / 127]),
      hub: new Float32Array(3),
      cards: parseScanCards(
        [
          { no: "BP01-001", c: "a", n: 1, nm: "a", a: "x", k: "elf", f: 0 },
          { no: "BP01-002", c: "b", n: 2, nm: "b", a: "y", k: "elf", f: 0 },
          { no: "BP01-001SL", c: "a", n: 3, nm: "a", a: "z", k: "elf", f: 0 },
        ],
        3,
      ),
      scoring: SCORING,
    }
    const mask = Uint8Array.from([0, 1, 1])
    const found = matchCards(tied, [unit(1, 0, 0, 0)], mask, 2)
    expect(found.map((match) => match.card.cardId)).toEqual(["b", "a"])
    expect(matchCards(tied, [unit(1, 0, 0, 0)], mask, 1).map((match) => match.card.cardId)).toEqual(
      ["b"],
    )
  })
})
