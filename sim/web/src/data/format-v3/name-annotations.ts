import { canonicalText, type JsonObject, objectValue, stringValue, utf8 } from "./json"
import { digest } from "./sha256"
import { TextOwners } from "./text-owners"

export function wholeName(unit: JsonObject, concept: string): JsonObject {
  const length = Array.from(stringValue(unit["text"])).length
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

export function translationNameConcept(owners: TextOwners, value: JsonObject): string {
  const pointer = objectValue(value["source"])
  const source = owners.get(objectValue(pointer["owner"]))
  return stringValue(source.row["name_concept_id"])
}
