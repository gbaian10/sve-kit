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

class Parser {
  index = 0
  private readonly text: string

  constructor(text: string) {
    this.text = text
  }

  fail(detail: string): never {
    return fail("json", `${detail} at offset ${String(this.index)}`)
  }

  skipWhitespace(): void {
    for (;;) {
      const code = this.text.charCodeAt(this.index)
      if (code === 0x20 || code === 0x0a || code === 0x0d || code === 0x09) this.index += 1
      else return
    }
  }

  parseValue(): JsonValue {
    this.skipWhitespace()
    const char = this.text[this.index]
    switch (char) {
      case "{":
        return this.parseObject()
      case "[":
        return this.parseArray()
      case '"':
        return this.parseString()
      case "t":
        return this.parseLiteral("true", true)
      case "f":
        return this.parseLiteral("false", false)
      case "n":
        return this.parseLiteral("null", null)
      case undefined:
        return this.fail("unexpected end of input")
      default:
        if (char === "-" || (char >= "0" && char <= "9")) return this.parseNumber()
        return this.fail(`unexpected character ${JSON.stringify(char)}`)
    }
  }

  private parseLiteral<T extends JsonPrimitive>(literal: string, value: T): T {
    if (!this.text.startsWith(literal, this.index)) this.fail("unexpected literal")
    this.index += literal.length
    return value
  }

  private parseNumber(): number {
    const start = this.index
    if (this.text[this.index] === "-") this.index += 1
    const first = this.text[this.index]
    if (first === "0") {
      this.index += 1
    } else if (first !== undefined && first >= "1" && first <= "9") {
      while (this.index < this.text.length && this.isDigit(this.index)) this.index += 1
    } else {
      this.fail("invalid number")
    }
    const next = this.text[this.index]
    if (next === "." || next === "e" || next === "E") this.fail("floating point JSON is forbidden")
    const literal = this.text.slice(start, this.index)
    const digits = literal.startsWith("-") ? literal.slice(1) : literal
    if (digits.length > 16) this.fail("unsafe integer")
    const magnitude = Number(digits)
    if (magnitude > SAFE_INTEGER) this.fail("unsafe integer")
    // `-0` has no canonical form of its own; the byte-equality gate rejects it in payloads.
    return literal.startsWith("-") && magnitude !== 0 ? -magnitude : magnitude
  }

  private isDigit(index: number): boolean {
    const code = this.text.charCodeAt(index)
    return code >= 0x30 && code <= 0x39
  }

  private parseString(): string {
    this.index += 1
    const parts: string[] = []
    let start = this.index
    for (;;) {
      if (this.index >= this.text.length) this.fail("unterminated string")
      const code = this.text.charCodeAt(this.index)
      if (code === 0x22) {
        parts.push(this.text.slice(start, this.index))
        this.index += 1
        return parts.join("")
      }
      if (code < 0x20) this.fail("control character in string")
      if (code === 0x5c) {
        parts.push(this.text.slice(start, this.index))
        parts.push(this.parseEscape())
        start = this.index
        continue
      }
      this.index += 1
    }
  }

  private parseEscape(): string {
    const char = this.text[this.index + 1]
    this.index += 2
    switch (char) {
      case '"':
        return '"'
      case "\\":
        return "\\"
      case "/":
        return "/"
      case "b":
        return "\b"
      case "f":
        return "\f"
      case "n":
        return "\n"
      case "r":
        return "\r"
      case "t":
        return "\t"
      case "u": {
        const unit = this.parseHex4()
        if (unit >= 0xdc00 && unit <= 0xdfff) this.fail("unpaired low surrogate")
        if (unit < 0xd800 || unit > 0xdbff) return String.fromCharCode(unit)
        if (!this.text.startsWith("\\u", this.index)) this.fail("unpaired high surrogate")
        this.index += 2
        const low = this.parseHex4()
        if (low < 0xdc00 || low > 0xdfff) this.fail("unpaired high surrogate")
        return String.fromCharCode(unit, low)
      }
      default:
        return this.fail("invalid escape")
    }
  }

  private parseHex4(): number {
    const hex = this.text.slice(this.index, this.index + 4)
    if (!/^[0-9a-fA-F]{4}$/.test(hex)) this.fail("invalid unicode escape")
    this.index += 4
    return Number.parseInt(hex, 16)
  }

  private parseArray(): JsonValue[] {
    this.index += 1
    const result: JsonValue[] = []
    this.skipWhitespace()
    if (this.text[this.index] === "]") {
      this.index += 1
      return result
    }
    for (;;) {
      result.push(this.parseValue())
      this.skipWhitespace()
      const char = this.text[this.index]
      this.index += 1
      if (char === "]") return result
      if (char !== ",") this.fail("expected , or ]")
    }
  }

  private parseObject(): JsonObject {
    this.index += 1
    const result: JsonObject = {}
    this.skipWhitespace()
    if (this.text[this.index] === "}") {
      this.index += 1
      return result
    }
    for (;;) {
      this.skipWhitespace()
      if (this.text[this.index] !== '"') this.fail("expected string key")
      const key = this.parseString()
      this.skipWhitespace()
      if (this.text[this.index] !== ":") this.fail("expected :")
      this.index += 1
      const value = this.parseValue()
      if (Object.hasOwn(result, key)) this.fail(`duplicate key ${JSON.stringify(key)}`)
      // defineProperty keeps a "__proto__" key from touching the prototype.
      Object.defineProperty(result, key, {
        value,
        enumerable: true,
        writable: true,
        configurable: true,
      })
      this.skipWhitespace()
      const char = this.text[this.index]
      this.index += 1
      if (char === "}") return result
      if (char !== ",") this.fail("expected , or }")
    }
  }
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
  const parser = new Parser(text)
  const value = parser.parseValue()
  parser.skipWhitespace()
  if (parser.index !== text.length) parser.fail("trailing characters")
  return value
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
