// @vitest-environment node
import Ajv2020 from "ajv/dist/2020"
import { describe, expect, it } from "vitest"

import compiled from "#snapshot-conformance"

import positives from "../../../fixtures/schema-positive.json"
import { v2Fixture } from "../v2-fixture"
import { SnapshotError } from "./errors"
import { type JsonObject, type JsonValue, parseStrict, stringValue } from "./json"
import {
  columns,
  definition,
  descriptor,
  primaryKey,
  requiredTypes,
  rowType,
  SCHEMA_ID,
  schemaRoot,
  tables,
  validate,
} from "./schema"
import { SchemaValidator } from "./validator"

const validator = new SchemaValidator(schemaRoot, compiled, "v2")

function fixture(name: string): JsonValue {
  if (name === "schema-valid.json") return positives
  if (name === "schema-invalid.json")
    return (v2Fixture(name) as JsonObject[]).map((item) => ({
      ...item,
      schema: item["target"] ?? null,
    }))
  if (name.startsWith("payloads/")) {
    const role = name.slice("payloads/".length, -".json".length)
    const manifest = v2Fixture("manifest.json") as JsonObject
    const file = (manifest["files"] as JsonObject[]).find(
      (file) =>
        file["role"] === role ||
        ((role === "detail" || role === "history") && file["role"] === "text"),
    )
    if (!file) throw new Error("missing golden payload")
    return v2Fixture(`payloads/${stringValue(file["sha256"]).slice(7)}.json`)
  }
  return v2Fixture(name)
}

const ajv = new Ajv2020({ strict: false, allErrors: false })
ajv.addSchema(schemaRoot)
function ajvAccepts(name: string, value: JsonValue): boolean {
  const check = ajv.getSchema(`${SCHEMA_ID}#/$defs/${name}`)
  if (!check) throw new Error(`ajv cannot resolve ${name}`)
  return check(value) === true
}
function accepts(name: string, value: JsonValue): boolean {
  return validator.validate(name, value) === null
}
/** Both validators must agree, and the expectation must hold. */
function expectAccepted(name: string, value: JsonValue, expected: boolean, label: string): void {
  expect(accepts(name, value), `reader ${label}`).toBe(expected)
  expect(ajvAccepts(name, value), `ajv ${label}`).toBe(expected)
}

const valid = (fixture("schema-valid.json") as JsonObject[]).map((c) => ({
  schema: c["schema"] as string,
  value: c["value"] ?? null,
}))
const invalid = fixture("schema-invalid.json") as JsonObject[]

describe("contract schema", () => {
  it("loads with the expected identity and covers every collection", () => {
    expect(schemaRoot["$id"]).toBe(SCHEMA_ID)
    expect(tables()).toHaveLength(43)
    expect(new Set(valid.map((c) => c.schema))).toContain("card")
    for (const table of tables())
      expect(
        valid.some((c) => c.schema === table),
        table,
      ).toBe(true)
  })

  it("exposes the fixed accessors", () => {
    expect(columns("Section")).toEqual(["ordinal", "text_unit_id", "kind"])
    expect(rowType("printing", "detail")).toBe("printing_detail")
    expect(primaryKey("printing")).toEqual(["id"])
    expect(descriptor("Current")).toEqual({
      columns: ["region", "revision_id", "basis"],
      items: definition("Current")["x-types"],
    })
    expect([...requiredTypes("printing_bootstrap")].sort()).toEqual([
      "PrintingFaceBootstrap",
      "PrintingStamp",
    ])
    expect(() => definition("nope")).toThrow("unknown schema definition")
  })

  it.each(valid)("accepts the shared positive $schema, like ajv", (c) => {
    expectAccepted(c.schema, c.value, true, c.schema)
  })

  it("rejects every shared counterexample, like ajv", () => {
    for (const c of invalid) {
      const name = c["schema"] as string
      const raw = c["raw_json"]
      if (typeof raw === "string") {
        expect(() => parseStrict(raw), name).toThrow(SnapshotError)
        continue
      }
      expectAccepted(name, c["value"] ?? null, false, `${name} ${JSON.stringify(c["name"])}`)
    }
  })

  it("rejects short, long and nullable-omitted tuples and object key changes", () => {
    for (const c of valid) {
      const value = c.value
      if (Array.isArray(value)) {
        expectAccepted(c.schema, value.slice(0, -1), false, `${c.schema} short`)
        expectAccepted(c.schema, [...value, null], false, `${c.schema} long`)
        value.forEach((item, index) => {
          if (item === null) {
            const changed = value.filter((_, i) => i !== index)
            expectAccepted(c.schema, changed, false, `${c.schema} nullable ${String(index)}`)
          }
        })
      } else if (value !== null && typeof value === "object") {
        for (const key of Object.keys(value)) {
          const { [key]: _dropped, ...changed } = value
          expectAccepted(c.schema, changed, false, `${c.schema} without ${key}`)
        }
        expectAccepted(c.schema, { ...value, extra: null }, false, `${c.schema} extra`)
      }
    }
  })

  it("rejects unknown enum values in every fixed enum column", () => {
    for (const c of valid) {
      if (!Array.isArray(c.value)) continue
      const kinds = definition(c.schema)["x-types"] as JsonObject[]
      kinds.forEach((raw, index) => {
        const kind = (raw["nullable"] ?? raw) as JsonObject
        if (!("enum" in kind)) return
        const changed = [...(c.value as JsonValue[])]
        changed[index] = "unknown-contract-value"
        expectAccepted(c.schema, changed, false, `${c.schema}[${String(index)}]`)
      })
    }
  })

  it("accepts the golden manifest and payloads", () => {
    validate("Manifest", fixture("manifest.json"))
    validate("Config", fixture("payloads/config.json"))
    validate("Programs", fixture("payloads/programs.json"))
    validate("TextAll", fixture("text-all.json"))
    for (const name of ["bootstrap", "detail", "history", "images"]) {
      validate("Container", fixture(`payloads/${name}.json`))
    }
  })

  it("reports the failing path and keyword", () => {
    const manifest = fixture("manifest.json") as JsonObject
    const broken = { ...manifest, published_at: "2026-02-30T00:00:00Z" }
    expect(() => {
      validate("Manifest", broken)
    }).toThrow(/schema at published_at: .*pattern/)
    const failure = validator.validate("Section", [0, "t:ja:0", "rule", 9])
    expect(failure?.keyword).toBe("maxItems")
  })

  it("compiles $ref, if/then/else, not, contains and propertyNames the way ajv does", () => {
    const parameter = { parameters: [{ name: "x", uint: null, variables: [] }] }
    expectAccepted("ParameterSchema", parameter, false, "empty domains")
    expectAccepted(
      "ParameterSchema",
      { parameters: [{ name: "x", uint: null, variables: ["X"] }] },
      true,
      "variable",
    )
    expectAccepted(
      "Manifest",
      {
        ...(fixture("manifest.json") as JsonObject),
        required_capabilities: ["column-partition-v1"],
      },
      false,
      "contains",
    )
    const config = fixture("payloads/config.json") as JsonObject
    const endpoint = {
      game: "sv1",
      card_url_template: "https://example.invalid/{official_id}",
      language_map: { "zh-Hant": "tw" },
      status: "unknown",
      refresh_policy: "frozen",
    }
    expectAccepted("Config", { ...config, digital_endpoints: [endpoint] }, true, "language_map")
    expectAccepted(
      "Config",
      { ...config, digital_endpoints: [{ ...endpoint, language_map: { ZH: "tw" } }] },
      false,
      "propertyNames",
    )
  })
})
