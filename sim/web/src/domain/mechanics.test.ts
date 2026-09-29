import { describe, expect, it } from "vitest"

import { type MechanicFacts, triState } from "./mechanics"

const universe = new Set(["kw:a", "kw:b", "kw:c"])
const facts = (over: Partial<MechanicFacts> = {}): MechanicFacts => ({
  present: new Set(),
  coverage: undefined,
  enBlocked: false,
  ...over,
})

describe("triState", () => {
  it.each([
    ["present wins", facts({ present: new Set(["kw:a"]) }), "kw:a", "present"],
    ["no coverage row", facts(), "kw:a", "unknown"],
    [
      "complete_all",
      facts({
        coverage: {
          completeAll: true,
          completeMode: "include",
          completeIds: [],
          partialMode: "include",
          partialIds: [],
        },
      }),
      "kw:a",
      "absent",
    ],
    [
      "complete include lists it",
      facts({
        coverage: {
          completeAll: false,
          completeMode: "include",
          completeIds: ["kw:a"],
          partialMode: "include",
          partialIds: [],
        },
      }),
      "kw:a",
      "absent",
    ],
    [
      "complete include leaves it out",
      facts({
        coverage: {
          completeAll: false,
          completeMode: "include",
          completeIds: ["kw:b"],
          partialMode: "include",
          partialIds: [],
        },
      }),
      "kw:a",
      "unknown",
    ],
    [
      "complete exclude leaves it out",
      facts({
        coverage: {
          completeAll: false,
          completeMode: "exclude",
          completeIds: ["kw:a"],
          partialMode: "include",
          partialIds: [],
        },
      }),
      "kw:a",
      "unknown",
    ],
    [
      "complete exclude keeps it",
      facts({
        coverage: {
          completeAll: false,
          completeMode: "exclude",
          completeIds: ["kw:b"],
          partialMode: "include",
          partialIds: [],
        },
      }),
      "kw:a",
      "absent",
    ],
    [
      "partial include is still unknown",
      facts({
        coverage: {
          completeAll: false,
          completeMode: "include",
          completeIds: [],
          partialMode: "include",
          partialIds: ["kw:a"],
        },
      }),
      "kw:a",
      "unknown",
    ],
    [
      "partial exclude is still unknown",
      facts({
        coverage: {
          completeAll: false,
          completeMode: "include",
          completeIds: [],
          partialMode: "exclude",
          partialIds: ["kw:b"],
        },
      }),
      "kw:a",
      "unknown",
    ],
    [
      "another keyword present does not make this one absent",
      facts({ present: new Set(["kw:b"]) }),
      "kw:a",
      "unknown",
    ],
  ] as const)("%s", (_label, input, keyword, expected) => {
    expect(triState(input, keyword, universe, "jp")).toBe(expected)
  })

  it("drops the shared answer to unknown on an EN-blocked card seen from the English side", () => {
    const blocked = facts({
      present: new Set(["kw:a"]),
      enBlocked: true,
      coverage: {
        completeAll: true,
        completeMode: "include",
        completeIds: [],
        partialMode: "include",
        partialIds: [],
      },
    })
    expect(triState(blocked, "kw:a", universe, "en")).toBe("unknown")
    expect(triState(blocked, "kw:b", universe, "en")).toBe("unknown")
    expect(triState(blocked, "kw:a", universe, "jp")).toBe("present")
    expect(triState(blocked, "kw:a", universe, "en", "en_override")).toBe("present")
  })
})
