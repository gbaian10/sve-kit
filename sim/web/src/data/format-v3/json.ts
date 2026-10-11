import { fail } from "./errors"

type JsonPrimitive = null | boolean | number | string
export type JsonValue = JsonPrimitive | JsonValue[] | JsonObject
export interface JsonObject {
  [key: string]: JsonValue
}
export type JsonPath = readonly (string | number)[]

const SAFE_INTEGER = 9007199254740991

function isObject(value: JsonValue | undefined): value is JsonObject {
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

const decoder = new TextDecoder()
const encoder = new TextEncoder()

/** Parse published JSON; the exporter and publisher validate its format offline. */
export function parseJson(input: Uint8Array | string): JsonValue {
  try {
    return JSON.parse(typeof input === "string" ? input : decoder.decode(input)) as JsonValue
  } catch {
    return fail("json", "invalid snapshot JSON")
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
