import { canonicalText, isObject, type JsonObject, type JsonPath, type JsonValue } from "./json"

export interface SchemaFailure {
  readonly path: JsonPath
  readonly keyword: string
  readonly detail: string
}

type Check = (value: JsonValue, path: JsonPath) => SchemaFailure | null

// Every keyword the contract schema uses today. Anything else fails at load time, so a future
// schema cannot silently mean more than this reader checks.
const KEYWORDS = new Set([
  "$schema",
  "$id",
  "$comment",
  "$defs",
  "$ref",
  "type",
  "const",
  "enum",
  "minimum",
  "maximum",
  "minLength",
  "pattern",
  "properties",
  "required",
  "additionalProperties",
  "propertyNames",
  "prefixItems",
  "items",
  "minItems",
  "maxItems",
  "uniqueItems",
  "contains",
  "allOf",
  "anyOf",
  "oneOf",
  "not",
  "if",
  "then",
  "else",
  "x-columns",
  "x-types",
  "x-primary-key",
  "x-fragments",
  "x-tables",
])
const ANNOTATIONS = new Set(["x-columns", "x-types", "x-primary-key", "x-fragments", "x-tables"])
const REF_PREFIX = "#/$defs/"

function typeName(value: JsonValue): string {
  if (value === null) return "null"
  if (Array.isArray(value)) return "array"
  if (typeof value === "number") return Number.isInteger(value) ? "integer" : "number"
  return typeof value
}

function accepts(types: readonly string[], value: JsonValue): boolean {
  const actual = typeName(value)
  return types.some((type) => type === actual || (type === "number" && actual === "integer"))
}

function codePoints(text: string): number {
  let count = 0
  for (const _ of text) count += 1
  return count
}

const patterns = new Map<string, RegExp>()
function regexp(pattern: string): RegExp {
  let compiled = patterns.get(pattern)
  if (!compiled) {
    compiled = new RegExp(pattern, "u")
    patterns.set(pattern, compiled)
  }
  return compiled
}

function ok(): null {
  return null
}

/**
 * The subset of JSON Schema 2020-12 that the contract schema uses, interpreted directly from the
 * schema document, so the reader accepts and rejects exactly what the published schema says.
 */
export class SchemaValidator {
  private readonly defs: JsonObject
  private readonly cache = new Map<string, Check>()

  constructor(root: JsonObject) {
    const defs = root["$defs"]
    if (!isObject(defs)) throw new Error("schema has no $defs")
    this.defs = defs
    this.assertKnownKeywords(root)
  }

  definition(name: string): JsonObject {
    const def = this.defs[name]
    if (!isObject(def)) throw new Error(`unknown schema definition ${name}`)
    return def
  }

  validate(name: string, value: JsonValue): SchemaFailure | null {
    return this.checkDef(name)(value, [])
  }

  private assertKnownKeywords(node: JsonValue): void {
    if (!isObject(node)) {
      if (Array.isArray(node)) for (const item of node) this.assertKnownKeywords(item)
      return
    }
    for (const [keyword, value] of Object.entries(node)) {
      if (!KEYWORDS.has(keyword)) throw new Error(`unsupported schema keyword ${keyword}`)
      if (keyword === "const" || keyword === "enum" || ANNOTATIONS.has(keyword)) continue
      if (keyword === "properties" || keyword === "$defs") {
        if (isObject(value)) for (const sub of Object.values(value)) this.assertKnownKeywords(sub)
        continue
      }
      if (keyword === "required" || keyword === "type" || keyword === "pattern") continue
      this.assertKnownKeywords(value)
    }
  }

  private checkDef(name: string): Check {
    let check = this.cache.get(name)
    if (!check) {
      // Register a forwarding check first so self-referential definitions cannot recurse forever.
      let compiled: Check | undefined
      check = (value, path) => {
        compiled ??= this.compile(this.definition(name))
        return compiled(value, path)
      }
      this.cache.set(name, check)
    }
    return check
  }

  private compile(schema: JsonValue): Check {
    if (schema === true) return ok
    if (schema === false)
      return (_value, path) => ({ path, keyword: "", detail: "nothing allowed here" })
    if (!isObject(schema)) throw new Error("schema node must be an object or boolean")
    const checks: Check[] = []
    const add = (keyword: string, check: Check) => {
      checks.push((value, path) => {
        const failure = check(value, path)
        return failure ? { ...failure, keyword: failure.keyword || keyword } : null
      })
    }
    const fail = (path: JsonPath, detail: string): SchemaFailure => ({ path, keyword: "", detail })

    const ref = schema["$ref"]
    if (typeof ref === "string") {
      if (!ref.startsWith(REF_PREFIX)) throw new Error(`unsupported $ref ${ref}`)
      add("$ref", this.checkDef(ref.slice(REF_PREFIX.length)))
    }
    const type = schema["type"]
    if (type !== undefined) {
      const types = (Array.isArray(type) ? type : [type]).map(String)
      add("type", (value, path) =>
        accepts(types, value)
          ? null
          : fail(path, `expected ${types.join("|")}, got ${typeName(value)}`),
      )
    }
    if (Object.hasOwn(schema, "const")) {
      const expected = canonicalText(schema["const"] ?? null)
      add("const", (value, path) =>
        canonicalText(value) === expected ? null : fail(path, `expected ${expected}`),
      )
    }
    const options = schema["enum"]
    if (Array.isArray(options)) {
      const allowed = new Set(options.map(canonicalText))
      add("enum", (value, path) =>
        allowed.has(canonicalText(value)) ? null : fail(path, "value not in enum"),
      )
    }
    const minimum = schema["minimum"]
    if (typeof minimum === "number") {
      add("minimum", (value, path) =>
        typeof value === "number" && value < minimum
          ? fail(path, `below ${String(minimum)}`)
          : null,
      )
    }
    const maximum = schema["maximum"]
    if (typeof maximum === "number") {
      add("maximum", (value, path) =>
        typeof value === "number" && value > maximum
          ? fail(path, `above ${String(maximum)}`)
          : null,
      )
    }
    const minLength = schema["minLength"]
    if (typeof minLength === "number") {
      add("minLength", (value, path) =>
        typeof value === "string" && codePoints(value) < minLength ? fail(path, "too short") : null,
      )
    }
    const pattern = schema["pattern"]
    if (typeof pattern === "string") {
      const compiled = regexp(pattern)
      add("pattern", (value, path) =>
        typeof value === "string" && !compiled.test(value) ? fail(path, "pattern mismatch") : null,
      )
    }
    this.compileObjectKeywords(schema, add, fail)
    this.compileArrayKeywords(schema, add, fail)
    this.compileCombinators(schema, add, fail)
    return (value, path) => {
      for (const check of checks) {
        const failure = check(value, path)
        if (failure) return failure
      }
      return null
    }
  }

  private compileObjectKeywords(
    schema: JsonObject,
    add: (keyword: string, check: Check) => void,
    fail: (path: JsonPath, detail: string) => SchemaFailure,
  ): void {
    const properties = schema["properties"]
    const known = new Map<string, Check>()
    if (isObject(properties)) {
      for (const [key, sub] of Object.entries(properties)) known.set(key, this.compile(sub))
    }
    const additional = schema["additionalProperties"]
    const extra = additional === undefined ? null : this.compile(additional)
    if (known.size > 0 || extra) {
      add("properties", (value, path) => {
        if (!isObject(value)) return null
        for (const [key, item] of Object.entries(value)) {
          const check = known.get(key) ?? extra
          if (!check) continue
          const failure = check(item, [...path, key])
          if (failure) return failure
        }
        return null
      })
    }
    const required = schema["required"]
    if (Array.isArray(required)) {
      const keys = required.map(String)
      add("required", (value, path) => {
        if (!isObject(value)) return null
        for (const key of keys) if (!Object.hasOwn(value, key)) return fail(path, `missing ${key}`)
        return null
      })
    }
    const names = schema["propertyNames"]
    if (names !== undefined) {
      const check = this.compile(names)
      add("propertyNames", (value, path) => {
        if (!isObject(value)) return null
        for (const key of Object.keys(value)) {
          const failure = check(key, [...path, key])
          if (failure) return failure
        }
        return null
      })
    }
  }

  private compileArrayKeywords(
    schema: JsonObject,
    add: (keyword: string, check: Check) => void,
    fail: (path: JsonPath, detail: string) => SchemaFailure,
  ): void {
    const prefix = schema["prefixItems"]
    const positional = Array.isArray(prefix) ? prefix.map((sub) => this.compile(sub)) : []
    const items = schema["items"]
    const rest = items === undefined ? null : this.compile(items)
    if (positional.length > 0 || rest) {
      add("items", (value, path) => {
        if (!Array.isArray(value)) return null
        for (const [index, item] of value.entries()) {
          const check = positional[index] ?? rest
          if (!check) continue
          const failure = check(item, [...path, index])
          if (failure) return failure
        }
        return null
      })
    }
    const minItems = schema["minItems"]
    if (typeof minItems === "number") {
      add("minItems", (value, path) =>
        Array.isArray(value) && value.length < minItems ? fail(path, "too few items") : null,
      )
    }
    const maxItems = schema["maxItems"]
    if (typeof maxItems === "number") {
      add("maxItems", (value, path) =>
        Array.isArray(value) && value.length > maxItems ? fail(path, "too many items") : null,
      )
    }
    if (schema["uniqueItems"] === true) {
      add("uniqueItems", (value, path) => {
        if (!Array.isArray(value)) return null
        const seen = new Set(value.map(canonicalText))
        return seen.size === value.length ? null : fail(path, "duplicate items")
      })
    }
    const contains = schema["contains"]
    if (contains !== undefined) {
      const check = this.compile(contains)
      add("contains", (value, path) =>
        !Array.isArray(value) || value.some((item, index) => !check(item, [...path, index]))
          ? null
          : fail(path, "no item matches contains"),
      )
    }
  }

  private compileCombinators(
    schema: JsonObject,
    add: (keyword: string, check: Check) => void,
    fail: (path: JsonPath, detail: string) => SchemaFailure,
  ): void {
    const branches = (keyword: string): Check[] => {
      const list = schema[keyword]
      return Array.isArray(list) ? list.map((sub) => this.compile(sub)) : []
    }
    const all = branches("allOf")
    if (all.length > 0) {
      add("allOf", (value, path) => {
        for (const check of all) {
          const failure = check(value, path)
          if (failure) return failure
        }
        return null
      })
    }
    const any = branches("anyOf")
    if (any.length > 0) {
      add("anyOf", (value, path) =>
        any.some((check) => !check(value, path)) ? null : fail(path, "no anyOf branch matches"),
      )
    }
    const one = branches("oneOf")
    if (one.length > 0) {
      add("oneOf", (value, path) => {
        const matches = one.filter((check) => !check(value, path)).length
        return matches === 1 ? null : fail(path, `${String(matches)} oneOf branches match`)
      })
    }
    const not = schema["not"]
    if (not !== undefined) {
      const check = this.compile(not)
      add("not", (value, path) => (check(value, path) ? null : fail(path, "matches not")))
    }
    const condition = schema["if"]
    if (condition !== undefined) {
      const test = this.compile(condition)
      const then = schema["then"] === undefined ? ok : this.compile(schema["then"])
      const otherwise = schema["else"] === undefined ? ok : this.compile(schema["else"])
      add("if", (value, path) => (test(value, path) ? otherwise(value, path) : then(value, path)))
    }
  }
}
