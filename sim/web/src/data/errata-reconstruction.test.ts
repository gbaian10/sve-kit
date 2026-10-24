import { describe, expect, it } from "vitest"

import { reconstructPrintedWording } from "./errata-reconstruction"

const change = (before: string, after: string) => ({ field: "effect", before, after })

describe("printed wording reconstruction", () => {
  it("retains the whole effect and marks only the replaced passage", () => {
    expect(
      reconstructPrintedWording(
        "First ability.\nDraw two.\nLast ability.",
        [change("Draw one.", "Draw two.")],
        true,
      ),
    ).toEqual({
      state: "derived_from_errata",
      before: [
        { text: "First ability.\n", changed: false },
        { text: "Draw ", changed: false },
        { text: "one", changed: true },
        { text: ".", changed: false },
        { text: "\nLast ability.", changed: false },
      ],
      after: "First ability.\nDraw two.\nLast ability.",
    })
  })

  it("keeps printing unknown without confirmed applicability even if the old wording is unique", () => {
    const result = reconstructPrintedWording("Draw two.", [change("Draw one.", "Draw two.")], false)
    expect(result.state).toBe("unknown")
    expect(result.before?.map(({ text }) => text).join("")).toBe("Draw one.")
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
    expect(result.before?.map(({ text }) => text).join("")).toBe("😀 Old A\nOld B  ")
    expect(result.after).toBe("😀 New A\nNew B  ")
    expect(result.state).toBe("derived_from_errata")
  })

  it("converts only the notice's JP icon wrapper, without normalizing the current text", () => {
    const result = reconstructPrintedWording(
      "{ファンファーレ}New。",
      [change("{《ファンファーレ》}Old。", "{《ファンファーレ》}New。")],
      true,
    )
    expect(result.before?.map(({ text }) => text).join("")).toBe("{ファンファーレ}Old。")
    expect(result.before?.filter(({ changed }) => changed).map(({ text }) => text)).toEqual(["Old"])
    expect(result.after).toBe("{ファンファーレ}New。")
    expect(
      reconstructPrintedWording(
        "{ファンファーレ}New。",
        [change("《ファンファーレ》Old。", "《ファンファーレ》New。")],
        true,
      ).before,
    ).toEqual([
      { text: "{ファンファーレ}", changed: false },
      { text: "Old", changed: true },
      { text: "。", changed: false },
    ])
  })

  it("does not split a changed icon marker or a Unicode scalar across deletion spans", () => {
    const result = reconstructPrintedWording(
      "Pay {コスト2}. 😀",
      [change("Pay {コスト1}. 😃", "Pay {コスト2}. 😀")],
      true,
    )
    expect(result.before?.filter(({ changed }) => changed).map(({ text }) => text)).toEqual([
      "{コスト1}. 😃",
    ])
    expect(result.before?.map(({ text }) => text).join("")).toBe("Pay {コスト1}. 😃")
  })
})
