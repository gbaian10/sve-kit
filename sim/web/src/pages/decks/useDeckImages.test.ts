import { renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { ImageIndex } from "../../data"
import type { DeckEntry } from "../../domain/deck/types"
import type { DeckFacts } from "./deck-info"
import { useDeckImages } from "./useDeckData"

// Each request answers with the phase the test sets for it; none brings image rows.
const phases = new Map<string, string>()
const requested: string[][] = []
vi.mock("../../app/snapshot", () => ({
  useRecentSummaries: (ids: readonly string[]) => {
    requested.push([...ids])
    return { phase: phases.get(ids[0] ?? "") ?? "pending", images: undefined }
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
const fallback: ImageIndex = {
  cardImage: () => undefined,
  asset: () => undefined,
  known: (printingId) => printingId === "p1",
}

describe("useDeckImages", () => {
  it("asks for all 61 printings of a full deck and settles a printing its failed request held", () => {
    phases.clear()
    requested.length = 0
    const { result, rerender } = renderHook(() => useDeckImages(deck, facts, fallback))
    expect(requested.slice(-4).map((ids) => ids.length)).toEqual([20, 20, 20, 1])
    // The main-thread index already has a base printing; the last one waits for its request.
    expect(result.current.pending("p1", "f")).toBe(false)
    expect(result.current.pending("p60", "f")).toBe(true)
    phases.set("p60", "error")
    rerender()
    expect(result.current.pending("p60", "f")).toBe(false)
    expect(result.current.pending("p59", "f")).toBe(true)
  })

  it("does not wait for printings past the requests", () => {
    phases.clear()
    const big = { ...deck, main: entries(1, 90), evolve: [] }
    const { result } = renderHook(() => useDeckImages(big, facts, fallback))
    expect(result.current.pending("p79", "f")).toBe(true)
    expect(result.current.pending("p80", "f")).toBe(false)
  })
})
