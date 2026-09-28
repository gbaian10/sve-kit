import schemaText from "../../../../../carddb/src/sve_carddb/snapshot/schema/v1/contract.schema.json?raw"
import { fail } from "./errors"
import {
  arrayValue,
  type JsonObject,
  type JsonPath,
  type JsonValue,
  objectValue,
  parseStrict,
  stringValue,
} from "./json"
import { SchemaValidator } from "./validator"

export const SCHEMA_ID = "urn:sve-kit:snapshot:1.0.0"

/** The published contract schema, read through the same strict JSON boundary as snapshot data. */
export const schemaRoot: JsonObject = objectValue(parseStrict(schemaText))
export const validator = new SchemaValidator(schemaRoot)

export function definition(name: string): JsonObject {
  return validator.definition(name)
}

/** Shape and primitive boundaries against a fixed definition; throws a `schema` error. */
export function validate(name: string, value: JsonValue, path: JsonPath = []): void {
  const failure = validator.validate(name, value)
  if (failure)
    fail("schema", `${name}: ${failure.keyword} ${failure.detail}`, [...path, ...failure.path])
}

function strings(value: JsonValue): string[] {
  return arrayValue(value).map((item) => stringValue(item))
}

/** The immutable column order of a row or nested tuple. */
export function columns(name: string): string[] {
  return strings(definition(name)["x-columns"] ?? null)
}

/** The complete public collection whitelist, in schema order. */
export function tables(): string[] {
  return strings(definition("Container")["x-tables"] ?? null)
}

/** A fragment's fixed row type, e.g. printing + detail -> printing_detail. */
export function rowType(table: string, partition: string): string {
  const mapping = objectValue(definition("Container")["x-fragments"] ?? null)
  return stringValue(objectValue(mapping[table] ?? null)[partition] ?? null)
}

/** The format-authoritative descriptor; payload descriptors must equal it, never replace it. */
export function descriptor(name: string): JsonObject {
  const def = definition(name)
  return { columns: def["x-columns"] ?? null, items: def["x-types"] ?? null }
}

export function primaryKey(table: string): string[] {
  return strings(definition(table)["x-primary-key"] ?? null)
}

function references(kind: JsonObject, into: Set<string>): void {
  const ref = kind["ref"]
  if (typeof ref === "string") {
    into.add(ref)
    for (const name of requiredTypes(ref)) into.add(name)
    return
  }
  for (const key of ["array", "nullable"]) {
    const inner = kind[key]
    if (inner !== undefined) references(objectValue(inner), into)
  }
}

/** Every nested descriptor a row type needs, from the fixed acyclic schema. */
export function requiredTypes(name: string): Set<string> {
  const result = new Set<string>()
  for (const item of arrayValue(definition(name)["x-types"] ?? null))
    references(objectValue(item), result)
  return result
}
