// @vitest-environment node
import * as jsonc from "jsonc-parser"
import { afterEach, describe, expect, it, vi } from "vitest"

import vectorsText from "../../../fixtures/canonical-vectors.json?raw"
import { SnapshotError } from "./errors"
import {
  canonical,
  canonicalText,
  compareBytes,
  compareCodePoints,
  type JsonObject,
  parseStrict,
  utf8,
} from "./json"
import { hex } from "./sha256"

const vectors = parseStrict(vectorsText) as JsonObject

afterEach(() => vi.restoreAllMocks())

function rejects(input: Uint8Array | string): SnapshotError {
  try {
    parseStrict(input)
  } catch (error) {
    expect(error).toBeInstanceOf(SnapshotError)
    return error as SnapshotError
  }
  throw new Error(`accepted ${JSON.stringify(input)}`)
}

describe("parseStrict", () => {
  it("parses nested values with whitespace", () => {
    expect(parseStrict(' {"a" : [1, -2, "x\\n", true, null, {}] , "b": {"c": []}} ')).toEqual({
      a: [1, -2, "x\n", true, null, {}],
      b: { c: [] },
    })
  })

  it("keeps astral characters and escapes intact", () => {
    expect(parseStrict('"\\ud83d\\ude00 😀 \\u00e9 \\/"')).toBe("😀 😀 é /")
  })

  it("treats -0 as 0", () => {
    expect(Object.is(parseStrict("-0"), 0)).toBe(true)
    expect(parseStrict("[-0]")).toEqual([0])
  })

  it.each([
    '{"a":1,"a":2}',
    "1.0",
    "1e3",
    "NaN",
    "Infinity",
    "-",
    "01",
    "9007199254740992",
    "-9007199254740992",
    "99999999999999999999",
    '"\\ud800"',
    '"\\udc00"',
    '"\\ud800x"',
    '"\\ud800\\u0041"',
    '"a\nb"',
    '"\\x"',
    '"\\u12"',
    "[1,]",
    "{}{}",
    "",
    '{"a" 1}',
    "[1 2]",
    "tru",
  ])("rejects %s", (input) => {
    expect(rejects(input).code).toBe("json")
  })

  it.each([
    '{"a":1,"\\u0061":2}',
    '{"x":{"a":1,"a":2}}',
    '{"x":[{"a":1,"a":2}]}',
    '{"\\ud800":1}',
    "/*comment*/{}",
    '{"a":/*comment*/1}',
    "{}//comment",
    '{"a":1,}',
    "[1,]",
    " \t\r\n ",
    "0.0",
    "1E0",
    "-0e0",
    "1e999",
  ])("rejects visitor policy and strict grammar counterexample %s", (input) => {
    expect(rejects(input).code).toBe("json")
  })

  it("keeps repeated keys in different objects and number-like strings", () => {
    expect(parseStrict('{"a":1,"x":{"a":2},"rows":[{"a":3},{"a":4}],"label":"1e0"}')).toEqual({
      a: 1,
      x: { a: 2 },
      rows: [{ a: 3 }, { a: 4 }],
      label: "1e0",
    })
    expect(parseStrict("[9007199254740991,-9007199254740991]")).toEqual([
      9007199254740991, -9007199254740991,
    ])
  })

  it("disables comments, trailing commas and empty content in the upstream visitor", () => {
    const visitor = vi.spyOn(jsonc, "visit")
    expect(parseStrict("{}")).toEqual({})
    expect(visitor).toHaveBeenCalledWith("{}", expect.any(Object), {
      disallowComments: true,
      allowTrailingComma: false,
      allowEmptyContent: false,
    })
  })

  it.each([
    jsonc.ParseErrorCode.InvalidSymbol,
    jsonc.ParseErrorCode.InvalidNumberFormat,
    jsonc.ParseErrorCode.PropertyNameExpected,
    jsonc.ParseErrorCode.ValueExpected,
    jsonc.ParseErrorCode.ColonExpected,
    jsonc.ParseErrorCode.CommaExpected,
    jsonc.ParseErrorCode.CloseBraceExpected,
    jsonc.ParseErrorCode.CloseBracketExpected,
    jsonc.ParseErrorCode.EndOfFileExpected,
    jsonc.ParseErrorCode.InvalidCommentToken,
    jsonc.ParseErrorCode.UnexpectedEndOfComment,
    jsonc.ParseErrorCode.UnexpectedEndOfString,
    jsonc.ParseErrorCode.UnexpectedEndOfNumber,
    jsonc.ParseErrorCode.InvalidUnicode,
    jsonc.ParseErrorCode.InvalidEscapeCharacter,
    jsonc.ParseErrorCode.InvalidCharacter,
  ])("rejects every upstream parser error (%s), even on otherwise valid JSON", (code) => {
    vi.spyOn(jsonc, "visit").mockImplementation((_text, visitor) => {
      visitor.onError?.(code, 0, 1, 0, 0)
    })
    expect(rejects("null").code).toBe("json")
  })

  it.each(["\u00a0", "\v", "\f", "\u2028", "\u2029"])(
    "rejects non-JSON spacing between tokens (%j)",
    (space) => {
      expect(rejects(`[1,${space}2]`).code).toBe("json")
    },
  )

  it("rejects lone surrogates in string input too", () => {
    expect(rejects('"\ud800"').code).toBe("json")
    expect(rejects('{"\ud800":1}').code).toBe("json")
    expect(rejects('"\udc00x"').code).toBe("json")
  })

  it("rejects a byte order mark and invalid UTF-8 bytes", () => {
    expect(rejects(new Uint8Array([0xef, 0xbb, 0xbf, 0x7b, 0x7d])).code).toBe("json")
    expect(rejects(new Uint8Array([0x22, 0xff, 0x22])).code).toBe("json")
    expect(rejects(new Uint8Array([0x22, 0xed, 0xa0, 0x80, 0x22])).code).toBe("json")
  })

  it("does not let a __proto__ key touch the prototype", () => {
    const value = parseStrict('{"__proto__":{"polluted":true},"x":1}') as JsonObject
    expect(Object.keys(value)).toEqual(["__proto__", "x"])
    expect(({} as { polluted?: boolean }).polluted).toBeUndefined()
    expect(canonicalText(value)).toBe('{"__proto__":{"polluted":true},"x":1}')
  })
})

describe("canonical", () => {
  it("matches the shared vectors byte for byte", () => {
    for (const raw of vectors["canonical"] as JsonObject[]) {
      expect(canonicalText(raw["value"] ?? null)).toBe(raw["text"])
    }
    const vector = vectors["bucket"] as JsonObject
    expect(hex(canonical(vector["key"] ?? null))).toBe(vector["bytes_hex"])
  })

  it("escapes only control characters, quotes and backslashes", () => {
    expect(canonicalText('a"b\\c\u007f\u2028/é😀\u001f')).toBe(
      '"a\\"b\\\\c\u007f\u2028/é😀\\u001f"',
    )
  })

  it("sorts keys by code point, not UTF-16 code unit", () => {
    expect(canonicalText({ "😀": 1, "\ue000": 2, z: 3, "": 4 })).toBe(
      '{"":4,"z":3,"\ue000":2,"😀":1}',
    )
  })

  it("rejects floats, unsafe integers and lone surrogates", () => {
    expect(() => canonical(1.5)).toThrow(SnapshotError)
    expect(() => canonical(2 ** 53)).toThrow(SnapshotError)
    expect(() => canonical("\ud800")).toThrow(SnapshotError)
    expect(canonicalText(-0)).toBe("0")
  })

  it("round-trips its own output", () => {
    const text = '{"a":[1,{"b":null}],"c":"x"}'
    expect(canonicalText(parseStrict(text))).toBe(text)
    expect(canonical(parseStrict(utf8(text)))).toEqual(utf8(text))
  })
})

describe("ordering", () => {
  it("compares strings by code point", () => {
    const sorted = ["😀", "\ue000", "z", "", "Z", "a"].sort(compareCodePoints)
    expect(sorted).toEqual(["", "Z", "a", "z", "\ue000", "😀"])
    expect(compareCodePoints("ab", "abc")).toBe(-1)
    expect(compareCodePoints("b", "abc")).toBe(1)
  })

  it("compares bytes as unsigned lexicographic values", () => {
    expect(compareBytes(new Uint8Array([1, 0xff]), new Uint8Array([1, 0x7f]))).toBe(1)
    expect(compareBytes(new Uint8Array([1]), new Uint8Array([1, 0]))).toBe(-1)
    expect(compareBytes(new Uint8Array([2]), new Uint8Array([2]))).toBe(0)
  })
})
