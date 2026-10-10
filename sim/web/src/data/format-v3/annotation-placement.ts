import { fail } from "./errors"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./json"
import { wholeName } from "./name-annotations"
import type { Fragment, View } from "./reader"

function displayRevisions(view: View): Set<JsonValue> {
  return new Set<JsonValue>(
    (view["face"] ?? [])
      .flatMap((face) => [
        ...arrayValue(face["current"]).map((value) => objectValue(value)["revision_id"]),
        ...arrayValue(face["wording"]).map(
          (value) => objectValue(objectValue(value)["display"])["revision_id"],
        ),
      ])
      .filter((value) => value !== null && value !== undefined),
  )
}

export function annotationLocations(view: View, fragments: readonly Fragment[]) {
  const display = displayRevisions(view)
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
  return (owner: JsonObject, field: string, ordinal: JsonValue = null) => {
    const kind = stringValue(owner["kind"])
    let home: JsonValue = null
    let entity: JsonValue[] = [owner, field, ordinal]
    if (kind === "face_revision") {
      const revision = revisions.get(stringValue(owner["id"]))
      const face = faces.get(stringValue(revision?.["face_id"]))
      const card = cards.get(stringValue(face?.["card_id"]))
      if (!card) fail("public-annotation/owner", "missing annotation owner card")
      home = card["home_set_id"] ?? null
      entity = [card["id"] ?? null]
    } else if (kind === "printing_face") {
      if (!printingHomes.has(stringValue(owner["id"])))
        fail("public-annotation/owner", "missing printing placement")
      home = printingHomes.get(stringValue(owner["id"])) ?? null
      entity = [owner["id"] ?? null]
    }
    const partition =
      kind === "face_revision" && !display.has(owner["id"] ?? null)
        ? "history"
        : (kind === "face_revision" && field === "name") ||
            ["keyword", "vocabulary", "product", "product_family"].includes(kind)
          ? "bootstrap"
          : "detail"
    return {
      physical: { kind: home === null ? "global" : "home_set", id: home },
      entity,
      partition,
    } as const
  }
}

export function validateAnnotationPlacement(view: View, fragments: readonly Fragment[]): void {
  const locate = annotationLocations(view, fragments)
  const annotations = new Set<JsonValue>()
  for (const fragment of fragments) {
    const part = fragment.value["partition"]
    if (fragment.table === "field_annotation")
      for (const row of fragment.rows) {
        const owner = objectValue(row["owner"])
        const location = locate(owner, stringValue(row["field"]), row["ordinal"] ?? null)
        if (canonicalText(fragment.value["owner"] ?? null) !== canonicalText(location.physical))
          fail("public-annotation/owner", "annotation fragment owner differs from source owner")
        const expected = location.partition
        if (part !== expected)
          fail("fragment-profile", "original annotation column partition mismatch")
        if (part === "bootstrap") annotations.add(row["annotation_set_id"] ?? null)
      }
    if (fragment.table === "translation" && part === "bootstrap")
      for (const row of fragment.rows)
        if (row["annotation_set_id"] !== null) annotations.add(row["annotation_set_id"] ?? null)
  }
  const texts = new Map((view["text_unit"] ?? []).map((row) => [row["id"], row]))
  for (const row of view["face_revision"] ?? []) {
    if (row["name_concept_id"] == null) continue
    const location = locate({ kind: "face_revision", id: row["id"] ?? null }, "name")
    const unit = texts.get(row["name_unit_id"])
    if (location.partition === "bootstrap" && unit)
      annotations.add(wholeName(unit, stringValue(row["name_concept_id"]))["id"] ?? null)
  }
  const concepts = new Set<JsonValue>()
  for (const row of view["annotation_set"] ?? [])
    if (annotations.has(row["id"] ?? null))
      for (const occurrence of arrayValue(row["occurrences"])) {
        const ref = objectValue(objectValue(occurrence)["reference"])
        if (ref["kind"] !== "vocabulary")
          concepts.add(ref[ref["kind"] === "card_name" ? "term_id" : "key"] ?? null)
      }
  const display = displayRevisions(view)
  for (const row of view["face_revision"] ?? [])
    if (display.has(row["id"] ?? null) && row["name_concept_id"] != null)
      concepts.add(row["name_concept_id"])
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
