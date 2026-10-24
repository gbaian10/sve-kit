import { describe, expect, it } from "vitest"

import { reconstructPrintedWording } from "./errata-reconstruction"

const change = (before: string, after: string) => ({ field: "effect", before, after })

describe("printed wording reconstruction", () => {
  it("does not delete the retained wording between two insertions", () => {
    const before = "このターン中にプレイした枚数"
    const after = "このターン中に自分がプレイしたカードの枚数"
    const result = reconstructPrintedWording(after, [change(before, after)], false)
    expect(result.diff?.removed.length).toBe(0)
  })

  it("retains the whole effect and marks only the replaced passage", () => {
    expect(
      reconstructPrintedWording(
        "First ability.\nDraw two.\nLast ability.",
        [change("Draw one.", "Draw two.")],
        true,
      ),
    ).toEqual({
      state: "derived_from_errata",
      before: "First ability.\nDraw one.\nLast ability.",
      diff: { removed: [{ start: 20, end: 23 }], added: [{ start: 20, end: 23 }] },
      after: "First ability.\nDraw two.\nLast ability.",
    })
  })

  it("keeps printing unknown without confirmed applicability even if the old wording is unique", () => {
    const result = reconstructPrintedWording("Draw two.", [change("Draw one.", "Draw two.")], false)
    expect(result.state).toBe("unknown")
    expect(result.before).toBe("Draw one.")
  })

  it.each([
    ["Draw two. Draw two.", [change("Draw one.", "Draw two.")], "ambiguous"],
    ["aaa", [change("old", "aa")], "ambiguous"],
    ["abcd", [change("old", "abc"), change("other", "bcd")], "ambiguous"],
    ["Draw three.", [change("Draw one.", "Draw two.")], "fragment_missing"],
    ["Draw two. Draw one.", [change("Draw one.", "Draw two.")], "roundtrip_failed"],
    ["Draw two. ", [change("Draw one.", "Draw two.\n")], "fragment_missing"],
    ["Text", [change("", "Text")], "unsupported"],
    ["Text", [change("Old", "")], "unsupported"],
    ["Text", [{ field: "name", before: "Old", after: "Text" }], "unsupported"],
    ["Text", [], "unsupported"],
    [undefined, [change("Old", "New")], "source_missing"],
  ] as const)("refuses unsafe reconstruction case %#", (text, changes, reason) => {
    expect(reconstructPrintedWording(text, changes, true)).toEqual({ state: "unknown", reason })
  })

  it("applies independent passages simultaneously and preserves Unicode and exact whitespace", () => {
    const result = reconstructPrintedWording(
      "😀 New A\nNew B  ",
      [change("Old B", "New B"), change("Old A", "New A")],
      true,
    )
    expect(result.before).toBe("😀 Old A\nOld B  ")
    expect(result.after).toBe("😀 New A\nNew B  ")
    expect(result.state).toBe("derived_from_errata")
  })

  it("converts only the notice's JP icon wrapper, without normalizing the current text", () => {
    const result = reconstructPrintedWording(
      "{ファンファーレ}New。",
      [change("{《ファンファーレ》}Old。", "{《ファンファーレ》}New。")],
      true,
    )
    expect(result.before).toBe("{ファンファーレ}Old。")
    expect(result.diff?.removed.map(({ start, end }) => result.before?.slice(start, end))).toEqual([
      "Old",
    ])
    expect(result.after).toBe("{ファンファーレ}New。")
    const bare = reconstructPrintedWording(
      "{ファンファーレ}New。",
      [change("《ファンファーレ》Old。", "《ファンファーレ》New。")],
      true,
    )
    expect(bare.before === "{ファンファーレ}Old。").toBe(true)
  })

  it("does not split a changed icon marker or a Unicode scalar across deletion spans", () => {
    const result = reconstructPrintedWording(
      "Pay {コスト2}. 😀",
      [change("Pay {コスト1}. 😃", "Pay {コスト2}. 😀")],
      true,
    )
    expect(result.diff?.removed.map(({ start, end }) => result.before?.slice(start, end))).toEqual([
      "{コスト1}",
      "😃",
    ])
    expect(result.before).toBe("Pay {コスト1}. 😃")
  })

  it("does not strike out unchanged old text when the correction only inserts text", () => {
    expect(
      reconstructPrintedWording(
        "Draw a card, then pass.",
        [change("Draw a card.", "Draw a card, then pass.")],
        true,
      ).diff?.removed.length,
    ).toBe(0)
  })
})
