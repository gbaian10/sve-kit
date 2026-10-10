import { fail } from "./errors"
import {
  arrayValue,
  canonicalText,
  integerValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./json"
import type { View } from "./reader"

export function annotationFailure(
  reason:
    | "duplicate_key"
    | "reference"
    | "ordering"
    | "range"
    | "owner"
    | "text_identity"
    | "emphasis"
    | "identity"
    | "basis",
): never {
  return fail(`public-annotation/${reason}`, reason)
}

const LABEL_FIELD = "label"

const FIELDS: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  face_revision: { name: "name_unit_id", effect: "effect_unit_id", section: "sections" },
  printing_face: {
    name: "printed_name_unit_id",
    effect: "printed_effect_unit_id",
    flavor: "flavor_unit_id",
    section: "sections",
  },
  qa_version: { question: "question_unit_id", answer: "answer_unit_id" },
  cr_clause: { effect: "text_unit_id" },
  vocabulary: { [LABEL_FIELD]: "label_unit_id" },
  product_family: { [LABEL_FIELD]: "name_unit_id" },
  product: { [LABEL_FIELD]: "name_unit_id" },
  keyword: { [LABEL_FIELD]: "name_unit_id", effect: "definition_unit_id", action_label: "actions" },
}

function ownerKey(owner: JsonObject): string {
  const kind = stringValue(owner["kind"])
  const keys =
    kind === "vocabulary"
      ? ["kind", "vocabulary_kind", "code"]
      : kind === "printing_face"
        ? ["kind", "id", "face_id"]
        : ["kind", "id"]
  if (
    !(kind in FIELDS) ||
    Object.keys(owner).length !== keys.length ||
    keys.some((k) => typeof owner[k] !== "string" || !owner[k])
  )
    annotationFailure("owner")
  return canonicalText(owner)
}

export interface TextOwner {
  readonly owner: JsonObject
  readonly row: JsonObject
  readonly cardId?: string
  readonly faceId?: string
  readonly region?: string
  readonly mappingState?: string
}

function mapping(card: JsonObject, region: string): string {
  const rows = arrayValue(card["regions"])
    .map((value) => objectValue(value))
    .filter((r) => r["region"] === region)
  if (rows.length !== 1) annotationFailure("owner")
  return stringValue(rows[0]?.["mapping_state"])
}

export class TextOwners {
  readonly byOwner = new Map<string, TextOwner>()

  constructor(view: View) {
    const cards = new Map((view["card"] ?? []).map((r) => [stringValue(r["id"]), r]))
    const faces = new Map((view["face"] ?? []).map((r) => [stringValue(r["id"]), r]))
    const add = (value: TextOwner): void => {
      const key = ownerKey(value.owner)
      if (this.byOwner.has(key)) annotationFailure("duplicate_key")
      this.byOwner.set(key, value)
    }
    for (const kind of ["qa_version", "cr_clause", "product_family", "product", "keyword"]) {
      for (const row of view[kind] ?? []) add({ owner: { kind, id: row["id"] ?? null }, row })
    }
    for (const row of view["vocabulary"] ?? [])
      add({
        owner: {
          kind: "vocabulary",
          vocabulary_kind: row["kind"] ?? null,
          code: row["code"] ?? null,
        },
        row,
      })
    for (const row of view["face_revision"] ?? []) {
      const face = faces.get(stringValue(row["face_id"]))
      const card = cards.get(stringValue(face?.["card_id"]))
      if (!face || !card) annotationFailure("owner")
      const region = stringValue(row["region"])
      add({
        owner: { kind: "face_revision", id: row["id"] ?? null },
        row,
        cardId: stringValue(card["id"]),
        faceId: stringValue(face["id"]),
        region,
        mappingState: mapping(card, region),
      })
    }
    for (const printing of view["printing"] ?? []) {
      const card = cards.get(stringValue(printing["card_id"]))
      if (!card) annotationFailure("owner")
      const region = stringValue(printing["region"])
      for (const value of arrayValue(printing["faces"])) {
        const row = objectValue(value)
        add({
          owner: {
            kind: "printing_face",
            id: printing["id"] ?? null,
            face_id: row["face_id"] ?? null,
          },
          row,
          cardId: stringValue(card["id"]),
          faceId: stringValue(row["face_id"]),
          region,
          mappingState: mapping(card, region),
        })
      }
    }
  }

  get(owner: JsonObject): TextOwner {
    const value = this.byOwner.get(ownerKey(owner))
    if (!value) annotationFailure("owner")
    return value
  }

  pointer(pointer: JsonObject): string {
    const value = this.optionalPointer(pointer)
    if (value === null) annotationFailure("owner")
    return value
  }

  optionalPointer(pointer: JsonObject): string | null {
    if (canonicalText(Object.keys(pointer).sort()) !== canonicalText(["field", "ordinal", "owner"]))
      annotationFailure("owner")
    const owner = this.get(objectValue(pointer["owner"]))
    const name = stringValue(pointer["field"])
    const ordinal = pointer["ordinal"]
    const key = FIELDS[stringValue(owner.owner["kind"])]?.[name]
    if (!key || ["section", "action_label"].includes(name) !== (ordinal !== null))
      annotationFailure("owner")
    let value: JsonValue | undefined = owner.row[key]
    if (name === "section") {
      const matches = arrayValue(value)
        .map((value) => objectValue(value))
        .filter((r) => r["ordinal"] === ordinal)
      if (matches.length !== 1) annotationFailure("owner")
      value = matches[0]?.["text_unit_id"]
    } else if (name === "action_label") {
      const index = integerValue(ordinal)
      if (index < 0 || index >= arrayValue(value).length) annotationFailure("owner")
      value = objectValue(arrayValue(value)[index])["label_unit_id"]
    }
    if (value === undefined) annotationFailure("owner")
    if (value === null) return null
    return stringValue(value)
  }
}
