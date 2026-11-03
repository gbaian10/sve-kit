import { describe, expect, it } from "vitest"

import type { ConstructionIndex, IndexCard } from "./construction"
import { createDeck } from "./model"
import { deckProblems } from "./rules"
import type { Deck } from "./types"

// The rules themselves are covered by the shared cases (construction.cases.test.ts); this checks
// only how the builder's deck reaches them and how their results read in the panel.
const cards = new Map<string, IndexCard>([
  ["c:leader", { name: 0, classCode: "elf", titles: [], zone: "leader", startAmulet: false }],
  ["c:rose", { name: 1, classCode: "elf", titles: [], zone: "main", startAmulet: false }],
  ["c:rose-alt", { name: 1, classCode: "elf", titles: [], zone: "main", startAmulet: false }],
])
const index: ConstructionIndex = {
  card: (id) => cards.get(id),
  titleRules: new Map(),
  formats: new Map([["standard", { coverage: [["2026-10-26", "2026-10-27"]], restrictions: [] }]]),
}
const names: Record<string, string> = { "c:rose": "ローズクイーン", "c:rose-alt": "ローズクイーン" }

function deck(change: Partial<Deck>): Deck {
  return {
    ...createDeck(
      { name: "D", edition: "jp", basis: { kind: "class", classCode: "elf" } },
      new Date("2026-11-03T00:00:00.000Z"),
    ),
    ...change,
  }
}

describe("deckProblems", () => {
  it("passes the deck through and names the first card of each problem", () => {
    const problems = deckProblems(
      deck({
        leader: { cardId: "c:leader", printingId: "p:leader-alt" },
        main: [
          { cardId: "c:rose", count: 2, printingId: "p:rose-sl" },
          { cardId: "c:rose-alt", count: 2 },
        ],
      }),
      index,
      "2026-10-26",
      (cardId) => names[cardId],
    )
    expect(problems.map((problem) => [problem.kind, problem.severity, problem.clause])).toEqual([
      ["mainTooFew", "error", "6.1.1.2"],
      ["sameNameOver", "error", "6.1.1.4"],
    ])
    expect(problems[1]).toMatchObject({
      cardIds: ["c:rose", "c:rose-alt"],
      values: { name: "ローズクイーン", section: "main", count: 4, limit: 3 },
    })
  })

  it("checks the deck's own format", () => {
    const problems = deckProblems(
      deck({ format: "crossover", leader: { cardId: "c:leader" } }),
      index,
      "2026-10-26",
      () => undefined,
    )
    expect(problems.map((problem) => [problem.kind, problem.clause])).toEqual([
      ["formatUnsupported", "format"],
    ])
  })

  it("reports checks it could not make as infos after the errors", () => {
    const problems = deckProblems(
      deck({ leader: { cardId: "c:leader" } }),
      index,
      "2026-11-03",
      () => undefined,
    )
    expect(problems.map((problem) => [problem.kind, problem.severity])).toEqual([
      ["mainTooFew", "error"],
      ["restrictionsUnknown", "info"],
    ])
  })
})
