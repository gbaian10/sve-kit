import { describe, expect, it } from "vitest"

import { v3Fixture } from "../v3-fixture"
import { validateAnnotations } from "./annotations"
import { arrayValue, objectValue, stringValue } from "./json"
import type { View } from "./reader"

const cases = arrayValue(v3Fixture("public-annotation-reader.json")).map((value) =>
  objectValue(value),
)

describe("independent public annotation reader scenarios", () => {
  it.each(cases)("$id", (scenario) => {
    const view: View = Object.fromEntries(
      Object.entries(objectValue(scenario["view"])).map(([key, rows]) => [
        key,
        arrayValue(rows).map((value) => objectValue(value)),
      ]),
    )
    const languages = arrayValue(scenario["languages"]).map((value) => stringValue(value))
    const expected = objectValue(scenario["expected"])
    if (expected["result"] === "reject")
      expect(() => {
        validateAnnotations(view, languages)
      }).toThrow(`public-annotation/${stringValue(expected["reason"])}`)
    else
      expect(() => {
        validateAnnotations(view, languages)
      }).not.toThrow()
  })
})
