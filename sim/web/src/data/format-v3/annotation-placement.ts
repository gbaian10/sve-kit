import { fail } from "./errors"
import { arrayValue, canonicalText, type JsonValue, objectValue, stringValue } from "./json"
import type { Fragment, View } from "./reader"

export function validateAnnotationPlacement(view: View, fragments: readonly Fragment[]): void {
  const display = new Set<JsonValue>(
    (view["face"] ?? [])
      .flatMap((face) => [
        ...arrayValue(face["current"]).map((value) => objectValue(value)["revision_id"]),
        ...arrayValue(face["wording"]).map(
          (value) => objectValue(objectValue(value)["display"])["revision_id"],
        ),
      ])
      .filter((value) => value !== null && value !== undefined),
  )
  const revisions = new Map(
    (view["face_revision"] ?? []).map((row) => [stringValue(row["id"]), row]),
  )
  const faces = new Map((view["face"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const cards = new Map((view["card"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const printingHomes = new Map(
    fragments
      .filter((f) => f.table === "printing" && f.value["base"] === null)
      .flatMap((f) =>
        f.rows.map((row) => [stringValue(row["id"]), objectValue(f.value["owner"])["id"]] as const),
      ),
  )
  const annotations = new Set<JsonValue>()
  for (const fragment of fragments) {
    const part = fragment.value["partition"]
    if (fragment.table === "field_annotation")
      for (const row of fragment.rows) {
        const owner = objectValue(row["owner"])
        let home: JsonValue = null
        if (owner["kind"] === "face_revision") {
          const revision = revisions.get(stringValue(owner["id"]))
          const face = faces.get(stringValue(revision?.["face_id"]))
          home = cards.get(stringValue(face?.["card_id"]))?.["home_set_id"] ?? null
        } else if (owner["kind"] === "printing_face")
          home = printingHomes.get(stringValue(owner["id"])) ?? null
        if (
          canonicalText(fragment.value["owner"] ?? null) !==
          canonicalText({ kind: home === null ? "global" : "home_set", id: home })
        )
          fail("public-annotation/owner", "annotation fragment owner differs from source owner")
        const expected =
          owner["kind"] === "face_revision" && !display.has(owner["id"] ?? null)
            ? "history"
            : (owner["kind"] === "face_revision" && row["field"] === "name") ||
                ["keyword", "vocabulary", "product", "product_family"].includes(
                  stringValue(owner["kind"]),
                )
              ? "bootstrap"
              : "detail"
        if (part !== expected)
          fail("fragment-profile", "original annotation column partition mismatch")
        if (part === "bootstrap") annotations.add(row["annotation_set_id"] ?? null)
      }
    if (fragment.table === "translation" && part === "bootstrap")
      for (const row of fragment.rows)
        if (row["annotation_set_id"] !== null) annotations.add(row["annotation_set_id"] ?? null)
  }
  const concepts = new Set<JsonValue>()
  for (const row of view["annotation_set"] ?? [])
    if (annotations.has(row["id"] ?? null))
      for (const occurrence of arrayValue(row["occurrences"])) {
        const ref = objectValue(objectValue(occurrence)["reference"])
        if (ref["kind"] !== "vocabulary")
          concepts.add(ref[ref["kind"] === "card_name" ? "term_id" : "key"] ?? null)
      }
  for (const fragment of fragments)
    if (["annotation_set", "annotation_concept"].includes(fragment.table)) {
      const selected = fragment.table === "annotation_set" ? annotations : concepts
      if (
        fragment.rows.some(
          (row) =>
            fragment.value["partition"] !==
            (selected.has(row["id"] ?? null) ? "bootstrap" : "detail"),
        )
      )
        fail("fragment-profile", "annotation bootstrap closure partition mismatch")
    }
}
