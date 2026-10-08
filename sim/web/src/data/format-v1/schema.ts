import compiled from "#snapshot-validators"

import contract from "../../../../../carddb/src/sve_carddb/contracts/schema/v2/contract.schema.json"
import { fail } from "./errors"
import {
  arrayValue,
  type JsonObject,
  type JsonPath,
  type JsonValue,
  objectValue,
  stringValue,
} from "./json"
import { SchemaValidator } from "./validator"

export const SCHEMA_ID = "urn:sve-kit:snapshot:2.0.0"

// A plain JSON import works in Vite, Vitest and Bun scripts alike; the file is our own published
// resource, so the strict byte boundary that snapshot data goes through is not needed here.
export const schemaRoot: JsonObject = objectValue(contract)
const validator = new SchemaValidator(schemaRoot, compiled, "v2")

function forVersion(version: string): SchemaValidator {
  if (version !== "2.0.0") fail("unsupported-version", "unsupported schema profile")
  return validator
}

export function definition(name: string, version = "2.0.0"): JsonObject {
  return forVersion(version).definition(name)
}

/** Shape and primitive boundaries against a fixed definition; throws a `schema` error. */
export function validate(
  name: string,
  value: JsonValue,
  path: JsonPath = [],
  version = "2.0.0",
): void {
  const selected = forVersion(version)
  const failure = selected.validate(name, value)
  if (failure)
    fail("schema", `${name}: ${failure.keyword} ${failure.detail}`, [...path, ...failure.path])
}

function strings(value: JsonValue): string[] {
  return arrayValue(value).map((item) => stringValue(item))
}

/** The immutable column order of a row or nested tuple. */
export function columns(name: string, version = "2.0.0"): string[] {
  return strings(definition(name, version)["x-columns"] ?? null)
}

/** The complete public collection whitelist, in schema order. */
export function tables(): string[] {
  return strings(definition("Container")["x-tables"] ?? null)
}

/** A fragment's fixed row type, e.g. printing + detail -> printing_detail. */
export function rowType(table: string, partition: string, version = "2.0.0"): string {
  const selected = forVersion(version)
  const mapping = objectValue(selected.definition("Container")["x-fragments"] ?? null)
  return stringValue(objectValue(mapping[table] ?? null)[partition] ?? null)
}

/** The format-authoritative descriptor; payload descriptors must equal it, never replace it. */
export function descriptor(name: string, version = "2.0.0"): JsonObject {
  const def = definition(name, version)
  return { columns: def["x-columns"] ?? null, items: def["x-types"] ?? null }
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
export function requiredTypes(name: string, version = "2.0.0"): Set<string> {
  const result = new Set<string>()
  for (const item of arrayValue(definition(name, version)["x-types"] ?? null))
    references(objectValue(item), result, version)
  return result
}
