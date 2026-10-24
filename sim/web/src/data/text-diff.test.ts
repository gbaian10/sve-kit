import { expect, it } from "vitest"

import { textDiff } from "./text-diff"

const passages = (before: string, after: string) => {
  const diff = textDiff(before, after)
  return {
    removed: diff.removed.map(({ start, end }) => before.slice(start, end)),
    added: diff.added.map(({ start, end }) => after.slice(start, end)),
  }
}

it("keeps retained text between independent insertions and deletions", () => {
  expect(passages("甲乙丙丁", "甲新乙丙增丁")).toEqual({ removed: [], added: ["新", "增"] })
  expect(passages("甲新乙丙增丁", "甲乙丙丁")).toEqual({ removed: ["新", "增"], added: [] })
})

it("pairs replacements while leaving their shared context alone", () => {
  expect(passages("Pay one, then draw two.", "Pay two, then draw three.")).toEqual({
    removed: ["one", "two"],
    added: ["two", "three"],
  })
})

it("keeps icons, combining sequences and emoji indivisible", () => {
  expect(passages("{コスト1} é 👩‍👩‍👧", "{コスト2} ê 👩‍👩‍👦")).toEqual({
    removed: ["{コスト1}", "é", "👩‍👩‍👧"],
    added: ["{コスト2}", "ê", "👩‍👩‍👦"],
  })
})

it("preserves exact whitespace and handles an empty side", () => {
  expect(passages("A\nB  ", "A\n B ")).toEqual({ removed: [" "], added: [" "] })
  expect(passages("", "New")).toEqual({ removed: [], added: ["New"] })
  expect(passages("Old", "")).toEqual({ removed: ["Old"], added: [] })
  expect(passages("Same", "Same")).toEqual({ removed: [], added: [] })
})
