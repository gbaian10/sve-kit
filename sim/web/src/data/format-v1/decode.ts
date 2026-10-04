import {
  arrayValue,
  type JsonObject,
  type JsonPath,
  type JsonValue,
  objectValue,
  stringValue,
} from "./json"
import { columns, definition } from "./schema"

export type Row = JsonObject

function decodeType(
  kind: JsonObject,
  value: JsonValue,
  path: JsonPath,
  version: string,
): JsonValue {
  const nullable = kind["nullable"]
  if (nullable !== undefined)
    return value === null ? null : decodeType(objectValue(nullable), value, path, version)
  const array = kind["array"]
  if (array !== undefined) {
    const inner = objectValue(array)
    return arrayValue(value, path).map((item, index) =>
      decodeType(inner, item, [...path, index], version),
    )
  }
  const ref = kind["ref"]
  if (ref !== undefined) return decodeRow(stringValue(ref), value, path, version)
  return value
}

/** A schema-validated tuple as an object keyed by the fixed column order, nested tuples included. */
export function decodeRow(
  name: string,
  value: JsonValue,
  path: JsonPath = [],
  version = "2.0.0",
): Row {
  const kinds = arrayValue(definition(name, version)["x-types"] ?? null)
  const names = columns(name, version)
  const cells = arrayValue(value, path)
  const row: Row = {}
  names.forEach((column, index) => {
    row[column] = decodeType(
      objectValue(kinds[index] ?? null),
      cells[index] ?? null,
      [...path, index],
      version,
    )
  })
  return row
}
