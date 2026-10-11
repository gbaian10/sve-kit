import contract from "../../../../../carddb/src/sve_carddb/contracts/schema/v3/contract.schema.json"
import { fail } from "./errors"
import { arrayValue, type JsonObject, type JsonValue, objectValue, stringValue } from "./json"

// Tuple decoding uses our fixed schema rather than descriptors supplied by the payload.
const schemaRoot: JsonObject = objectValue(contract)
const definitions = objectValue(schemaRoot["$defs"])

export function definition(name: string, version = "3.0.0"): JsonObject {
  if (version !== "3.0.0") fail("unsupported-version", "unsupported schema profile")
  const value = definitions[name]
  if (value === undefined) throw new Error(`unknown schema definition ${name}`)
  return objectValue(value)
}

function strings(value: JsonValue): string[] {
  return arrayValue(value).map((item) => stringValue(item))
}

/** The immutable column order of a row or nested tuple. */
export function columns(name: string, version = "3.0.0"): string[] {
  return strings(definition(name, version)["x-columns"] ?? null)
}

/** The complete public collection whitelist, in schema order. */
export function tables(): string[] {
  return strings(definition("Container")["x-tables"] ?? null)
}

/** A fragment's fixed row type, e.g. printing + detail -> printing_detail. */
export function rowType(table: string, partition: string, version = "3.0.0"): string {
  const mapping = objectValue(definition("Container", version)["x-fragments"] ?? null)
  return stringValue(objectValue(mapping[table] ?? null)[partition] ?? null)
}

export function primaryKey(table: string): string[] {
  return strings(definition(table)["x-primary-key"] ?? null)
}

function references(kind: JsonObject, into: Set<string>, version: string): void {
  const ref = kind["ref"]
  if (typeof ref === "string") {
    into.add(ref)
    for (const name of requiredTypes(ref, version)) into.add(name)
    return
  }
  for (const key of ["array", "nullable"]) {
    const inner = kind[key]
    if (inner !== undefined) references(objectValue(inner), into, version)
  }
}

/** Every nested descriptor a row type needs, from the fixed acyclic schema. */
export function requiredTypes(name: string, version = "3.0.0"): Set<string> {
  const result = new Set<string>()
  for (const item of arrayValue(definition(name, version)["x-types"] ?? null))
    references(objectValue(item), result, version)
  return result
}
