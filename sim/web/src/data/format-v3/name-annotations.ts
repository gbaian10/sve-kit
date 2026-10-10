import { arrayValue, canonicalText, type JsonObject, objectValue, stringValue, utf8 } from "./json"
import type { View } from "./reader"
import { digest } from "./sha256"
import { annotationFailure, type TextOwner, TextOwners } from "./text-owners"

export function wholeName(unit: JsonObject, concept: string): JsonObject {
  const length = Array.from(stringValue(unit["text"])).length
  if (!length) annotationFailure("range")
  const occurrences = [
    {
      ordinal: 0,
      reference: { kind: "card_name", term_id: concept },
      ranges: [{ start: 0, end: length }],
      bold: true,
    },
  ]
  return {
    id: `ann:${digest(utf8(canonicalText({ recipe: "annotation-v1", text_unit_id: unit["id"] ?? null, occurrences }))).slice(7)}`,
    text_unit_id: unit["id"] ?? null,
    occurrences,
  }
}

export function translationNameConcept(
  owners: TextOwners,
  receiver: TextOwner,
  value: JsonObject,
): string {
  const pointer = objectValue(value["source"])
  if (
    value["field"] !== "name" ||
    value["ordinal"] !== null ||
    pointer["field"] !== "name" ||
    pointer["ordinal"] !== null
  )
    annotationFailure("owner")
  const source = owners.get(objectValue(pointer["owner"]))
  const concept = source.row["name_concept_id"]
  if (
    !["face_revision", "printing_face"].includes(stringValue(source.owner["kind"])) ||
    typeof concept !== "string"
  )
    annotationFailure("owner")
  const receiving = receiver.row["name_concept_id"]
  if (receiving != null && receiving !== concept) annotationFailure("text_identity")
  return concept
}

/** Used only once the exact source closure is available; partial views stay pending until then. */
export function expandNames(view: View): void {
  const owners = new TextOwners(view)
  const texts = new Map((view["text_unit"] ?? []).map((row) => [row["id"], row]))
  const sets = new Map((view["annotation_set"] ?? []).map((row) => [row["id"], row]))
  const fields = new Map(
    (view["field_annotation"] ?? []).map((row) => [
      canonicalText([row["owner"] ?? null, row["field"] ?? null, row["ordinal"] ?? null]),
      row,
    ]),
  )
  const translations = new Map((view["translation"] ?? []).map((row) => [row["id"], row]))
  view["annotation_set"] ??= []
  view["field_annotation"] ??= []
  const add = (unitId: string, concept: string): JsonObject => {
    const unit = texts.get(unitId)
    if (!unit) annotationFailure("reference")
    const annotation = wholeName(unit, concept)
    const previous = sets.get(annotation["id"])
    if (previous && canonicalText(previous) !== canonicalText(annotation))
      annotationFailure("identity")
    if (!previous) {
      sets.set(annotation["id"], annotation)
      view["annotation_set"]?.push(annotation)
    }
    return annotation
  }
  for (const owner of owners.byOwner.values()) {
    const concept = owner.row["name_concept_id"]
    if (concept != null) {
      const annotation = add(
        owners.pointer({ owner: owner.owner, field: "name", ordinal: null }),
        stringValue(concept),
      )
      const key = canonicalText([owner.owner, "name", null])
      const previous = fields.get(key)
      if (previous && previous["annotation_set_id"] !== annotation["id"])
        annotationFailure("text_identity")
      if (!previous) {
        const field = {
          owner: owner.owner,
          field: "name",
          ordinal: null,
          annotation_set_id: annotation["id"] ?? null,
        }
        fields.set(key, field)
        view["field_annotation"].push(field)
      }
    }
    for (const raw of arrayValue(owner.row["translations"])) {
      const value = objectValue(raw),
        translation = translations.get(value["translation_id"])
      if (!translation || translation["annotation_kind"] !== "whole_name") continue
      const annotation = add(
        stringValue(translation["text_unit_id"]),
        translationNameConcept(owners, owner, value),
      )
      if (
        translation["annotation_set_id"] !== null &&
        translation["annotation_set_id"] !== annotation["id"]
      )
        annotationFailure("text_identity")
      translation["annotation_set_id"] = annotation["id"] ?? null
    }
  }
  for (const row of translations.values()) {
    if (
      !["none", "explicit", "whole_name"].includes(stringValue(row["annotation_kind"])) ||
      (row["annotation_kind"] === "none") !== (row["annotation_set_id"] === null)
    )
      annotationFailure("reference")
  }
}
