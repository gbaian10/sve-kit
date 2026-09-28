// @vitest-environment node
import { describe, expect, it } from "vitest"

import vectorsText from "../../../../../tests/fixtures/snapshot-contract/v1/vectors.json?raw"
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

function rejects(input: Uint8Array | string): string {
  try {
    parseStrict(input)
  } catch (error) {
    expect(error).toBeInstanceOf(SnapshotError)
    return (error as SnapshotError).message
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
    ['{"a":1,"a":2}', "duplicate"],
    ["1.0", "floating"],
    ["1e3", "floating"],
    ["NaN", "unexpected"],
    ["Infinity", "unexpected"],
    ["-", "invalid number"],
    ["01", "trailing"],
    ["9007199254740992", "unsafe"],
    ["-9007199254740992", "unsafe"],
    ["99999999999999999999", "unsafe"],
    ['"\\ud800"', "surrogate"],
    ['"\\udc00"', "surrogate"],
    ['"\\ud800x"', "surrogate"],
    ['"\\ud800\\u0041"', "surrogate"],
    ['"a\nb"', "control"],
    ['"\\x"', "escape"],
    ['"\\u12"', "unicode"],
    ["[1,]", "unexpected"],
    ["{}{}", "trailing"],
    ["", "end of input"],
    ['{"a" 1}', "expected :"],
    ["[1 2]", "expected , or ]"],
    ["tru", "literal"],
  ])("rejects %s", (input, detail) => {
    expect(rejects(input)).toContain(detail)
  })

  it("rejects lone surrogates in string input too", () => {
    expect(rejects('"\ud800"')).toContain("surrogate")
    expect(rejects('{"\ud800":1}')).toContain("surrogate")
    expect(rejects('"\udc00x"')).toContain("surrogate")
  })

  it("rejects a byte order mark and invalid UTF-8 bytes", () => {
    expect(rejects(new Uint8Array([0xef, 0xbb, 0xbf, 0x7b, 0x7d]))).toContain("unexpected")
    expect(rejects(new Uint8Array([0x22, 0xff, 0x22]))).toContain("UTF-8")
    expect(rejects(new Uint8Array([0x22, 0xed, 0xa0, 0x80, 0x22]))).toContain("UTF-8")
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
