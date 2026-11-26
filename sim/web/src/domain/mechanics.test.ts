import { expect, it } from "vitest"

import { type MechanicCoverage, mechanicKey, mechanicState, parseMechanicKey } from "./mechanics"

const complete = (changes: Partial<MechanicCoverage> = {}): MechanicCoverage => ({
  scope: "shared",
  completeAll: false,
  completeMode: "include",
  completeKeywordIds: [],
  ...changes,
})
it("distinguishes has, other relations, partial and unexamined coverage", () => {
  const has = [{ scope: "shared", keywordId: "kw:ward", relations: ["has"] }]
  expect(mechanicState("kw:ward", "has", "jp", has, [], false)).toBe("present")
  expect(mechanicState("kw:storm", "has", "jp", has, [], false)).toBe("unknown")
  expect(
    mechanicState(
      "kw:ward",
      "has",
      "jp",
      [{ scope: "shared", keywordId: "kw:ward", relations: ["grants"] }],
      [],
      false,
    ),
  ).toBe("unknown")
  expect(mechanicState("kw:ward", "has", "jp", [], [complete()], false)).toBe("unknown")
})
it("only complete include/exclude/all coverage proves absence", () => {
  const include = complete({ completeKeywordIds: ["kw:ward"] })
  expect(mechanicState("kw:ward", "has", "jp", [], [include], false)).toBe("absent")
  expect(mechanicState("kw:storm", "has", "jp", [], [include], false)).toBe("unknown")
  const exclude = complete({ completeMode: "exclude", completeKeywordIds: ["kw:ward"] })
  expect(mechanicState("kw:ward", "has", "jp", [], [exclude], false)).toBe("unknown")
  expect(mechanicState("kw:storm", "has", "jp", [], [exclude], false)).toBe("absent")
  expect(mechanicState("kw:ward", "has", "jp", [], [complete({ completeAll: true })], false)).toBe(
    "absent",
  )
})
it("blocks both shared present and absent for EN and respects an EN override", () => {
  const shared = [{ scope: "shared", keywordId: "kw:ward", relations: ["has"] }]
  expect(mechanicState("kw:ward", "has", "en", shared, [], true)).toBe("unknown")
  expect(mechanicState("kw:ward", "has", "en", [], [complete({ completeAll: true })], true)).toBe(
    "unknown",
  )
  expect(mechanicState("kw:ward", "has", "en", shared, [], false)).toBe("present")
  expect(
    mechanicState(
      "kw:ward",
      "has",
      "en",
      [...shared, { keywordId: "kw:ward", scope: "en_override", relations: ["refers"] }],
      [complete({ scope: "en_override", completeAll: true })],
      true,
    ),
  ).toBe("absent")
})

it("uses relation-specific has coverage without claiming four-relation completeness", () => {
  const coverage = [
    complete({ completeAll: false, completeKeywordIds: [], hasCompleteKeywordIds: ["kw:ward"] }),
  ]
  expect(mechanicState("kw:ward", "has", "jp", [], coverage, false)).toBe("absent")
  expect(mechanicState("kw:storm", "has", "jp", [], coverage, false)).toBe("unknown")
  expect(
    mechanicState(
      "kw:ward",
      "has",
      "en",
      [],
      [
        complete({
          completeAll: false,
          completeKeywordIds: [],
          hasCompleteKeywordIds: ["kw:ward"],
          scope: "en_override",
        }),
      ],
      true,
    ),
  ).toBe("absent")
})
it("judges each relation on its own and keeps the wording check to what a card holds", () => {
  const projections = [{ scope: "shared", keywordId: "kw:ward", relations: ["grants", "counts"] }]
  const checked = [complete({ completeKeywordIds: ["kw:ward"] })]
  expect(mechanicState("kw:ward", "grants", "jp", projections, checked, false)).toBe("present")
  expect(mechanicState("kw:ward", "counts", "jp", projections, checked, false)).toBe("present")
  expect(mechanicState("kw:ward", "refers", "jp", projections, checked, false)).toBe("absent")
  expect(mechanicState("kw:ward", "has", "jp", projections, checked, false)).toBe("absent")
  const wordingOnly = [complete({ hasCompleteKeywordIds: ["kw:ward"] })]
  expect(mechanicState("kw:ward", "has", "jp", [], wordingOnly, false)).toBe("absent")
  expect(mechanicState("kw:ward", "grants", "jp", [], wordingOnly, false)).toBe("unknown")
})
it("keeps a held keyword's key bare and rejects unknown relation suffixes", () => {
  expect(mechanicKey("has", "kw:ward")).toBe("kw:ward")
  expect(mechanicKey("grants", "kw:ward")).toBe("kw:ward.grants")
  expect(parseMechanicKey("kw:ward")).toEqual({ relation: "has", keywordId: "kw:ward" })
  expect(parseMechanicKey("kw:ward.counts")).toEqual({ relation: "counts", keywordId: "kw:ward" })
  expect(parseMechanicKey("kw:ward.has")).toBeUndefined()
  expect(parseMechanicKey("kw:ward.future")).toBeUndefined()
})
