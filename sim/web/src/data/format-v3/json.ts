import { printParseErrorCode, visit } from "jsonc-parser"

import { fail } from "./errors"

type JsonPrimitive = null | boolean | number | string
export type JsonValue = JsonPrimitive | JsonValue[] | JsonObject
export interface JsonObject {
  [key: string]: JsonValue
}
export type JsonPath = readonly (string | number)[]

const SAFE_INTEGER = 9007199254740991

export function isObject(value: JsonValue | undefined): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

export function objectValue(value: JsonValue | undefined, path: JsonPath = []): JsonObject {
  if (!isObject(value)) fail("shape", "expected object", path)
  return value
}

export function arrayValue(value: JsonValue | undefined, path: JsonPath = []): JsonValue[] {
  if (!Array.isArray(value)) fail("shape", "expected array", path)
  return value
}

export function stringValue(value: JsonValue | undefined, path: JsonPath = []): string {
  if (typeof value !== "string") fail("shape", "expected string", path)
  return value
}

/** Booleans are not integers, and nothing beyond 2^53-1 survives canonical bytes. */
export function integerValue(value: JsonValue | undefined, path: JsonPath = []): number {
  if (typeof value !== "number" || !Number.isInteger(value) || Math.abs(value) > SAFE_INTEGER) {
    fail("shape", "expected safe integer", path)
  }
  return value
}

function isSurrogate(unit: number): boolean {
  return unit >= 0xd800 && unit <= 0xdfff
}

/** Unicode code point order, which UTF-16 code unit order gets wrong above U+D7FF. */
export function compareCodePoints(a: string, b: string): number {
  const length = Math.min(a.length, b.length)
  for (let i = 0; i < length; i += 1) {
    const ca = a.charCodeAt(i)
    const cb = b.charCodeAt(i)
    if (ca === cb) continue
    const sa = isSurrogate(ca)
    const sb = isSurrogate(cb)
    if (sa !== sb) return sa ? 1 : -1
    return ca < cb ? -1 : 1
  }
  return Math.sign(a.length - b.length)
}

export function compareBytes(a: Uint8Array, b: Uint8Array): number {
  const length = Math.min(a.length, b.length)
  for (let i = 0; i < length; i += 1) {
    const x = a[i] ?? 0
    const y = b[i] ?? 0
    if (x !== y) return x < y ? -1 : 1
  }
  return Math.sign(a.length - b.length)
}

const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true })
const encoder = new TextEncoder()

/** The upstream visitor supplies decoded keys and token spans; policy does not rescan JSON. */
function checkJson(text: string): void {
  const objects: Set<string>[] = []
  const reject = (detail: string, offset: number): never =>
    fail("json", `${detail} at offset ${String(offset)}`)
  visit(
    text,
    {
      onObjectBegin: () => {
        objects.push(new Set())
      },
      onObjectEnd: () => {
        objects.pop()
      },
      onObjectProperty: (property, offset) => {
        if (!property.isWellFormed()) reject("lone surrogate in property", offset)
        const seen = objects.at(-1)
        if (!seen) return reject("unexpected property", offset)
        if (seen.has(property)) reject(`duplicate key ${JSON.stringify(property)}`, offset)
        seen.add(property)
      },
      onLiteralValue: (value: unknown, offset, length) => {
        if (typeof value === "string" && !value.isWellFormed())
          reject("lone surrogate in string", offset)
        if (typeof value === "number") {
          // The lexical spelling matters even when its numeric value would be an integer.
          if (/[.eE]/.test(text.slice(offset, offset + length)))
            reject("floating point JSON is forbidden", offset)
          if (!Number.isSafeInteger(value)) reject("unsafe integer", offset)
        }
      },
      onError: (error, offset) => {
        reject(`unexpected JSON syntax (${printParseErrorCode(error)})`, offset)
      },
    },
    { disallowComments: true, allowTrailingComma: false, allowEmptyContent: false },
  )
}

/** Decode UTF-8 JSON without duplicate keys, floats, unsafe integers, BOM or lone surrogates. */
export function parseStrict(input: Uint8Array | string): JsonValue {
  let text: string
  if (typeof input === "string") {
    // Bytes go through the fatal decoder; a JS string can carry lone surrogates on its own.
    if (!input.isWellFormed()) fail("json", "lone surrogate in input")
    text = input
  } else {
    try {
      text = decoder.decode(input)
    } catch {
      fail("json", "invalid UTF-8")
    }
  }
  checkJson(text)
  try {
    // Native JSON.parse safely creates own __proto__ properties; avoid the library's object builder.
    // Negative zero retains the existing canonical-json-v1 value semantics.
    return JSON.parse(text, (_key: string, value: unknown) =>
      value === 0 ? 0 : value,
    ) as JsonValue
  } catch {
    return fail("json", "unexpected JSON syntax")
  }
}

// eslint-disable-next-line no-control-regex -- canonical-json-v1 escapes exactly these control characters
const ESCAPED = /[\u0000-\u001f"\\]/g

function quoted(value: string): string {
  if (!value.isWellFormed()) fail("json", "lone surrogate in string")
  return `"${value.replace(ESCAPED, (char) => {
    if (char === '"') return '\\"'
    if (char === "\\") return "\\\\"
    return `\\u${char.charCodeAt(0).toString(16).padStart(4, "0")}`
  })}"`
}

function encode(value: JsonValue): string {
  if (value === null) return "null"
  if (typeof value === "boolean") return value ? "true" : "false"
  if (typeof value === "string") return quoted(value)
  if (typeof value === "number") {
    if (!Number.isInteger(value)) fail("json", "floating point JSON is forbidden")
    if (Math.abs(value) > SAFE_INTEGER) fail("json", "unsafe integer")
    return value === 0 ? "0" : String(value)
  }
  if (Array.isArray(value)) return `[${value.map(encode).join(",")}]`
  const keys = Object.keys(value).sort(compareCodePoints)
  return `{${keys.map((key) => `${quoted(key)}:${encode(value[key] ?? null)}`).join(",")}}`
}

/** canonical-json-v1 text (build-db.md §14): sorted keys, no whitespace, lowercase control escapes. */
export function canonicalText(value: JsonValue): string {
  return encode(value)
}

export function canonical(value: JsonValue): Uint8Array {
  return encoder.encode(encode(value))
}

export function utf8(text: string): Uint8Array {
  return encoder.encode(text)
}
