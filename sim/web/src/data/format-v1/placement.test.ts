// @vitest-environment node
import { describe, expect, it } from "vitest"

import { type JsonObject } from "./json"
import { validatePlacement } from "./placement"
import { type Fragment } from "./reader"
import { bucket } from "./sha256"

function fragment(
  table: string,
  partition: string,
  home: string | null,
  width: number,
  rows: JsonObject[] = [],
  number = 63,
): Fragment {
  const kind = home === null ? "global" : "home_set"
  const role = ["image_asset", "image_variant", "printing_image"].includes(table)
    ? "images"
    : partition === "bootstrap"
      ? "bootstrap"
      : "text"
  return {
    file: `${role}/${partition}/${kind}/${home ?? "global"}/band/${String(Math.floor(number / width))}`,
    table,
    identity: "synthetic",
    rows,
    value: { owner: { kind, id: home }, partition, bucket: number, base: null },
  }
}

describe("independent fixed placement matrix", () => {
  it.each([
    ["image_asset", "detail", null, 1],
    ["image_variant", "detail", null, 1],
    ["printing_image", "detail", "TEST", 64],
    ["vocabulary", "bootstrap", null, 8],
    ["card", "bootstrap", "TEST", 64],
    ["card", "bootstrap", "BP01", 32],
    ["printing", "bootstrap", "CP04", 32],
    ["rules_name", "detail", null, 2],
    ["printing", "detail", "TEST", 32],
    ["text_unit", "history", null, 32],
    ["face_revision", "history", "TEST", 32],
  ] as const)("%s/%s/%s must use width %i", (table, part, home, width) => {
    const valid = fragment(table, part, home, width)
    expect(() => {
      validatePlacement([valid], "1.1.0")
    }).not.toThrow()
    const incorrect = {
      ...valid,
      file: valid.file.replace(
        /band\/\d+$/u,
        `band/${String(Math.floor(63 / (width === 64 ? 32 : 64)))}`,
      ),
    }
    expect(() => {
      validatePlacement([incorrect], "1.1.0")
    }).toThrow("file does not match fixed band")
  })
  it.each([
    ["card", { id: "c:synthetic" }, "TEST"],
    ["printing", { id: "p:synthetic" }, "TEST"],
    ["art", { id: "a:synthetic" }, "TEST"],
    ["face", { id: "f:synthetic", card_id: "c:synthetic" }, "TEST"],
    ["image_asset", { id: "i:synthetic" }, null],
    ["image_variant", { image_id: "i:synthetic" }, null],
  ] as const)("rejects misplaced %s even with a correct File key", (table, row, home) => {
    const key = table === "face" ? row.card_id : table === "image_variant" ? row.image_id : row.id
    const part = ["image_asset", "image_variant"].includes(table) ? "detail" : "bootstrap"
    const width = home === null ? 1 : 64
    const correct = bucket([key], 64)
    expect(() => {
      validatePlacement([fragment(table, part, home, width, [row], correct)], "1.1.0")
    }).not.toThrow()
    expect(() => {
      validatePlacement([fragment(table, part, home, width, [row], (correct + 1) % 64)], "1.1.0")
    }).toThrow("entity bucket mismatch")
  })
})
