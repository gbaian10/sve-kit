import { renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { ImageIndex } from "../../data"
import type { DeckEntry } from "../../domain/deck/types"
import type { DeckFacts } from "./deck-info"
import { useDeckImages } from "./useDeckData"

// Each request answers as the test sets for its first printing; by default it is still running.
interface Answer {
  readonly phase: string
  readonly recent?: readonly { printingId: string; summary?: object }[]
  readonly images?: ImageIndex
}
const answers = new Map<string, Answer>()
const requested: string[][] = []
vi.mock("../../app/snapshot", () => ({
  useRecentSummaries: (ids: readonly string[]) => {
    requested.push([...ids])
    return answers.get(ids[0] ?? "") ?? { phase: "pending", images: undefined }
  },
  useCardSnapshot: () => undefined,
  useImageIndex: () => undefined,
}))

// Every card is its own printing, with one face.
const facts = {
  summaryOf: (cardId: string) => ({ printingId: cardId, faceId: "f" }),
} as unknown as DeckFacts
const entries = (from: number, count: number): DeckEntry[] =>
  Array.from({ length: count }, (_, i) => ({ cardId: `p${String(from + i)}`, count: 1 }))
// A leader, fifty main and ten evolve cards, all of different printings.
const deck = {
  edition: "jp" as const,
  leader: { cardId: "p0" },
  main: entries(1, 50),
  evolve: entries(51, 10),
  side: [],
}
// The main-thread index holds the base printing p1 only.
const fallback: ImageIndex = {
  cardImage: () => undefined,
  asset: (printingId) => (printingId === "p1" ? { availability: "ready" } : undefined),
}
const knowing = (printingIds: readonly string[]): ImageIndex => ({
  cardImage: () => undefined,
  asset: () => undefined,
  known: (printingId) => printingIds.includes(printingId),
})

describe("useDeckImages", () => {
  it("asks for every printing of a full deck and waits only on the ones still coming", () => {
    answers.clear()
    requested.length = 0
    const { result, rerender } = renderHook(() => useDeckImages(deck, facts, fallback))
    const all = Array.from({ length: 61 }, (_, i) => `p${String(i)}`)
    expect(new Set(requested.slice(-4).flat())).toEqual(new Set(all))
    expect(result.current.pending("p1", "f")).toBe(false)
    expect(result.current.pending("p60", "f")).toBe(true)
    // The last request comes back: p60's rows are fetched next, then it is settled.
    answers.set("p60", { phase: "complete", recent: [{ printingId: "p60", summary: {} }] })
    rerender()
    expect(result.current.pending("p60", "f")).toBe(true)
    answers.set("p60", {
      phase: "complete",
      recent: [{ printingId: "p60", summary: {} }],
      images: knowing(["p60"]),
    })
    rerender()
    expect(result.current.pending("p60", "f")).toBe(false)
  })

  it("settles a printing the worker lacks, and one whose own request failed, but no other", () => {
    answers.clear()
    answers.set("p0", {
      phase: "complete",
      recent: [{ printingId: "p2" }],
      images: knowing([]),
    })
    answers.set("p20", { phase: "error" })
    const { result } = renderHook(() => useDeckImages(deck, facts, fallback))
    expect(result.current.pending("p2", "f")).toBe(false)
    expect(result.current.pending("p25", "f")).toBe(false)
    // The third request is still running.
    expect(result.current.pending("p45", "f")).toBe(true)
  })

  it("does not wait for printings past the requests", () => {
    answers.clear()
    const big = { ...deck, main: entries(1, 90), evolve: [] }
    const { result } = renderHook(() => useDeckImages(big, facts, fallback))
    expect(result.current.pending("p79", "f")).toBe(true)
    expect(result.current.pending("p80", "f")).toBe(false)
  })
})
