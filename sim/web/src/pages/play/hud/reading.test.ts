import { describe, expect, it } from "vitest"

import type { Inspection } from "../../../battle/view/inspect"
import { costLine, kindOfCode, kindOfTerm, statsLine } from "./reading"

const state = (zone: Inspection["zone"], card: Partial<Inspection["card"]> = {}): Inspection => ({
  card: {
    handle: "a1",
    card: { printing_id: "p:test", face: 0 },
    face: { printing_id: "p:test", face: 0 },
    faceDown: false,
    acted: false,
    evolution: "base",
    stats: null,
    keywords: [],
    counters: [],
    cost: null,
    attack: null,
    ...card,
  },
  seat: "p1",
  zone,
  under: null,
  type: "フォロワー",
  traits: [],
})

describe("kindOfTerm", () => {
  it("reads the kind out of the rules term the authority reports", () => {
    expect(kindOfTerm("フォロワー")).toBe("follower")
    expect(kindOfTerm("フォロワー・エボルヴ")).toBe("follower")
    expect(kindOfTerm("スペル")).toBe("spell")
    expect(kindOfTerm("アミュレット")).toBe("amulet")
    expect(kindOfTerm("リーダー")).toBe("leader")
    // A term the screen has no word for is left to be written as it is.
    expect(kindOfTerm("トークン")).toBeNull()
  })

  it("reads the printed kind from the snapshot's code", () => {
    expect(kindOfCode("amulet")).toBe("amulet")
    expect(kindOfCode("evolve")).toBeNull()
    expect(kindOfCode(null)).toBeNull()
    expect(kindOfCode(undefined)).toBeNull()
  })
})

describe("costLine", () => {
  it("shows the authority's quote where cards are played from, with the printed cost kept apart", () => {
    expect(costLine(state("hand", { cost: 3 }), 5)).toEqual({ kind: "quoted", now: 3, printed: 5 })
    expect(costLine(state("ex", { cost: 0 }), 2)).toEqual({ kind: "quoted", now: 0, printed: 2 })
  })

  it("is unknown without a settled quote: the printed cost never takes its place", () => {
    expect(costLine(state("hand"), 5)).toEqual({ kind: "unknown", printed: 5 })
    expect(costLine(state("ex"), null)).toEqual({ kind: "unknown", printed: null })
  })

  it("is the printed cost, said to be printed, where nothing is quoted", () => {
    expect(costLine(state("field", { cost: 3 }), 5)).toEqual({ kind: "printed", printed: 5 })
    expect(costLine(state("cemetery"), 5)).toEqual({ kind: "printed", printed: 5 })
    expect(costLine(null, 5)).toEqual({ kind: "printed", printed: 5 })
    expect(costLine(null, null)).toBeNull()
  })
})

describe("statsLine", () => {
  it("shows the numbers the authority reports, whatever the card prints", () => {
    expect(statsLine(state("field", { stats: { attack: 2, life: 3 } }), 4, 6)).toEqual({
      kind: "current",
      attack: 2,
      defense: 3,
      printed: { attack: 4, defense: 6 },
    })
    // A card that prints no numbers and became a follower.
    expect(statsLine(state("field", { stats: { attack: 2, life: 3 } }), null, null)).toEqual({
      kind: "current",
      attack: 2,
      defense: 3,
      printed: null,
    })
  })

  it("shows the printed numbers as printed where the authority reports none", () => {
    expect(statsLine(state("field"), 4, 6)).toEqual({
      kind: "printed",
      attack: 4,
      defense: 6,
      printed: { attack: 4, defense: 6 },
    })
    expect(statsLine(state("hand"), 4, 6)?.kind).toBe("printed")
    expect(statsLine(null, 4, 6)?.kind).toBe("printed")
    expect(statsLine(state("field"), null, null)).toBeNull()
  })
})
