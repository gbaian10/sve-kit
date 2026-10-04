// @vitest-environment node
import { createRequire } from "node:module"
import { runInNewContext } from "node:vm"

import type { ValidateFunction } from "ajv"
import { describe, expect, it } from "vitest"

import { compileSchemas } from "./compile"

const authority = {
  $id: "urn:sve-kit:test:standalone",
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $ref: "#/$defs/Record",
  $defs: {
    Text: { type: "string", minLength: 2 },
    Record: {
      type: "object",
      properties: { label: { $ref: "#/$defs/Text" } },
      required: ["label"],
      additionalProperties: false,
      "x-columns": ["label"],
      "x-types": [{ string: true }],
      "x-primary-key": ["label"],
      "x-tables": ["synthetic"],
      "x-fragments": {},
    },
  },
}

describe("Ajv2020 standalone compilation", () => {
  it("registers explicit annotations and executes with string code generation forbidden", () => {
    const code = compileSchemas({ fixture: authority })
    const checks: Record<string, ValidateFunction> = {}
    runInNewContext(
      code,
      { exports: checks, require: createRequire(import.meta.url) },
      {
        contextCodeGeneration: { strings: false, wasm: false },
      },
    )
    const check = checks["fixture_Record"]
    if (!check) throw new Error("standalone export missing")
    expect(check({ label: "ok" })).toBe(true)
    expect(check({ label: "x" })).toBe(false)
    expect(check({ label: "ok", extra: true })).toBe(false)
    expect(code).not.toContain("new Function")
    expect(code).not.toContain("eval(")
  })

  it.each(["x-undeclared", "minLenght", "unexpectedKeyword"])(
    "rejects an unregistered schema keyword %s",
    (keyword) => {
      const defs = structuredClone(authority.$defs)
      const changed = {
        ...authority,
        $defs: { ...defs, Record: { ...defs.Record, [keyword]: true } },
      }
      expect(() => compileSchemas({ fixture: changed })).toThrow(/unknown keyword/)
      expect(() => compileSchemas({ fixture: { ...authority, [keyword]: true } })).toThrow(
        /unknown keyword/,
      )
    },
  )
})
