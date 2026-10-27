import { expect, it } from "vitest"

import { type MechanicCoverage, mechanicState } from "./mechanics"

const complete = (changes: Partial<MechanicCoverage> = {}): MechanicCoverage => ({
  scope: "shared",
  completeAll: false,
  completeMode: "include",
  completeKeywordIds: [],
  ...changes,
})
it("distinguishes has, other relations, partial and unexamined coverage", () => {
  const has = [{ scope: "shared", keywordId: "kw:ward", relations: ["has"] }]
  expect(mechanicState("kw:ward", "jp", has, [], false)).toBe("present")
  expect(mechanicState("kw:storm", "jp", has, [], false)).toBe("unknown")
  expect(
    mechanicState(
      "kw:ward",
      "jp",
      [{ scope: "shared", keywordId: "kw:ward", relations: ["grants"] }],
      [],
      false,
    ),
  ).toBe("unknown")
  expect(mechanicState("kw:ward", "jp", [], [complete()], false)).toBe("unknown")
})
it("only complete include/exclude/all coverage proves absence", () => {
  const include = complete({ completeKeywordIds: ["kw:ward"] })
  expect(mechanicState("kw:ward", "jp", [], [include], false)).toBe("absent")
  expect(mechanicState("kw:storm", "jp", [], [include], false)).toBe("unknown")
  const exclude = complete({ completeMode: "exclude", completeKeywordIds: ["kw:ward"] })
  expect(mechanicState("kw:ward", "jp", [], [exclude], false)).toBe("unknown")
  expect(mechanicState("kw:storm", "jp", [], [exclude], false)).toBe("absent")
  expect(mechanicState("kw:ward", "jp", [], [complete({ completeAll: true })], false)).toBe(
    "absent",
  )
})
it("blocks both shared present and absent for EN and respects an EN override", () => {
  const shared = [{ scope: "shared", keywordId: "kw:ward", relations: ["has"] }]
  expect(mechanicState("kw:ward", "en", shared, [], true)).toBe("unknown")
  expect(mechanicState("kw:ward", "en", [], [complete({ completeAll: true })], true)).toBe(
    "unknown",
  )
  expect(mechanicState("kw:ward", "en", shared, [], false)).toBe("present")
  expect(
    mechanicState(
      "kw:ward",
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
  expect(mechanicState("kw:ward", "jp", [], coverage, false)).toBe("absent")
  expect(mechanicState("kw:storm", "jp", [], coverage, false)).toBe("unknown")
  expect(
    mechanicState("kw:ward", "en", [], [{ ...coverage[0]!, scope: "en_override" }], true),
  ).toBe("absent")
})
