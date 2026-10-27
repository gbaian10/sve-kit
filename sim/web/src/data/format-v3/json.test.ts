// @vitest-environment node
import { describe, expect, it } from "vitest"

import vectorsText from "../../../fixtures/canonical-vectors.json?raw"
import { SnapshotError } from "./errors"
import {
  canonical,
  canonicalText,
  compareCodePoints,
  type JsonObject,
  parseJson,
  utf8,
} from "./json"
import { digest } from "./sha256"

const vectors = parseJson(vectorsText) as JsonObject

describe("parseJson", () => {
  it("parses published JSON bytes and nested values", () => {
    expect(parseJson(utf8('{"a":[1,"😀",true,null]}'))).toEqual({ a: [1, "😀", true, null] })
  })
  it.each(["{", "[1,]", "", "{}{}"])("reports invalid JSON syntax", (text) => {
    expect(() => parseJson(text)).toThrow("invalid snapshot JSON")
  })
  it("uses native number and object semantics", () => {
    expect(Object.is(parseJson("-0"), -0)).toBe(true)
    expect(parseJson('{"a":1,"a":2,"b":1e0}')).toEqual({ a: 2, b: 1 })
    const value = parseJson('{"__proto__":{"polluted":true}}') as JsonObject
    expect(Object.keys(value)).toEqual(["__proto__"])
    expect(({} as { polluted?: boolean }).polluted).toBeUndefined()
  })
})

describe("canonical", () => {
  it("matches the shared vectors byte for byte", () => {
    for (const raw of vectors["canonical"] as JsonObject[]) {
      expect(canonicalText(raw["value"] ?? null)).toBe(raw["text"])
    }
    const vector = vectors["bucket"] as JsonObject
    expect(digest(canonical(vector["key"] ?? null))).toBe(vector["sha256"] as string)
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
    expect(canonicalText(parseJson(text))).toBe(text)
    expect(canonical(parseJson(utf8(text)))).toEqual(utf8(text))
  })
})

describe("ordering", () => {
  it("compares strings by code point", () => {
    const sorted = ["😀", "\ue000", "z", "", "Z", "a"].sort(compareCodePoints)
    expect(sorted).toEqual(["", "Z", "a", "z", "\ue000", "😀"])
    expect(compareCodePoints("ab", "abc")).toBe(-1)
    expect(compareCodePoints("b", "abc")).toBe(1)
  })
})
