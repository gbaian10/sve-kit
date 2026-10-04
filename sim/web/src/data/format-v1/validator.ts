import type { ValidateFunction } from "ajv"

import { isObject, type JsonObject, type JsonPath, type JsonValue, objectValue } from "./json"

export interface SchemaFailure {
  readonly path: JsonPath
  readonly keyword: string
  readonly detail: string
}

/** Numeric object keys remain strings; only array indexes become numbers. */
function errorPath(pointer: string, value: JsonValue): JsonPath {
  const result: (string | number)[] = []
  let current: JsonValue | undefined = value
  for (const escaped of pointer.split("/").slice(1)) {
    const key = escaped.replace(/~1/g, "/").replace(/~0/g, "~")
    result.push(Array.isArray(current) ? Number(key) : key)
    current = Array.isArray(current)
      ? current[Number(key)]
      : isObject(current)
        ? current[key]
        : undefined
  }
  return result
}

/** Dispatch precompiled functions; the client has no schema compiler or keyword interpreter. */
export class SchemaValidator {
  private readonly definitions: JsonObject
  private readonly checks: Readonly<Record<string, ValidateFunction>>
  private readonly prefix: string

  constructor(
    root: JsonObject,
    checks: Readonly<Record<string, ValidateFunction>>,
    prefix: string,
  ) {
    this.definitions = objectValue(root["$defs"])
    this.checks = checks
    this.prefix = prefix
  }

  definition(name: string): JsonObject {
    const value = this.definitions[name]
    if (value === undefined) throw new Error(`unknown schema definition ${name}`)
    return objectValue(value)
  }

  validate(name: string, value: JsonValue): SchemaFailure | null {
    const check = this.checks[`${this.prefix}_${name}`]
    if (!check) throw new Error(`unknown schema definition ${name}`)
    if (check(value)) return null
    const error = check.errors?.[0]
    if (!error) throw new Error("compiled validator rejected without an error")
    const params = error.params as Record<string, unknown>
    const property = params["missingProperty"] ?? params["additionalProperty"]
    const path = errorPath(error.instancePath, value)
    return {
      path: typeof property === "string" ? [...path, property] : path,
      keyword: error.keyword,
      detail: error.message ?? "invalid value",
    }
  }
}
