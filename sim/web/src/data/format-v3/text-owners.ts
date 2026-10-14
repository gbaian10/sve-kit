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

export function annotationFailure(reason: "reference" | "owner"): never {
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

export interface TextOwner {
  readonly owner: JsonObject
  readonly row: JsonObject
  readonly region?: string
  readonly cardId?: string
}

export class TextOwners {
  readonly byOwner = new Map<string, TextOwner>()
  private readonly witnesses = new Map<string, JsonObject>()
  private readonly cards = new Map<string, JsonObject>()
  private readonly faces = new Map<string, JsonObject>()

  constructor(view: View) {
    this.include(view)
  }

  /** Indexes rows not seen before; a row that replaces an indexed one needs a new instance. */
  include(view: View): void {
    const { cards, faces } = this
    for (const row of view["card"] ?? []) cards.set(stringValue(row["id"]), row)
    for (const row of view["face"] ?? []) faces.set(stringValue(row["id"]), row)
    const add = (value: TextOwner): void => {
      this.byOwner.set(canonicalText(value.owner), value)
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
      if (!face) annotationFailure("owner")
      const card = cards.get(stringValue(face["card_id"]))
      if (!card) annotationFailure("owner")
      add({
        owner: { kind: "face_revision", id: row["id"] ?? null },
        row,
        cardId: stringValue(card["id"]),
        ...(typeof row["region"] === "string" ? { region: row["region"] } : {}),
      })
    }
    for (const printing of view["printing"] ?? []) {
      const card = cards.get(stringValue(printing["card_id"]))
      if (!card) annotationFailure("owner")
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
          ...(typeof printing["region"] === "string" ? { region: printing["region"] } : {}),
        })
      }
    }
  }

  includeNameSources(sources: readonly JsonObject[]): void {
    for (const source of sources) {
      const pointer = objectValue(source["pointer"])
      const key = canonicalText(pointer)
      const previous = this.witnesses.get(key)
      if (previous && canonicalText(previous) !== canonicalText(source)) annotationFailure("owner")
      const owner = this.byOwner.get(canonicalText(pointer["owner"] ?? null))
      if (
        owner &&
        (this.optionalPointer(pointer) !== source["text_unit_id"] ||
          (owner.row["name_concept_id"] ?? null) !== source["name_concept_id"])
      )
        annotationFailure("owner")
      this.witnesses.set(key, source)
    }
  }
  sourceNameConcept(pointer: JsonObject): string {
    const owner = this.byOwner.get(canonicalText(pointer["owner"] ?? null))
    if (owner) return stringValue(owner.row["name_concept_id"])
    return stringValue(this.witnesses.get(canonicalText(pointer))?.["name_concept_id"])
  }

  get(owner: JsonObject): TextOwner {
    const value = this.byOwner.get(canonicalText(owner))
    if (!value) annotationFailure("owner")
    return value
  }

  optionalPointer(pointer: JsonObject): string | null {
    const source = this.witnesses.get(canonicalText(pointer))
    if (!this.byOwner.has(canonicalText(pointer["owner"] ?? null)) && source)
      return stringValue(source["text_unit_id"])
    const owner = this.get(objectValue(pointer["owner"]))
    const name = stringValue(pointer["field"])
    const ordinal = pointer["ordinal"]
    const key = FIELDS[stringValue(owner.owner["kind"])]?.[name]
    if (!key) annotationFailure("owner")
    let value: JsonValue | undefined = owner.row[key]
    if (name === "section") {
      value = arrayValue(value)
        .map((value) => objectValue(value))
        .find((row) => row["ordinal"] === ordinal)?.["text_unit_id"]
    } else if (name === "action_label") {
      const index = integerValue(ordinal)
      value = objectValue(arrayValue(value)[index])["label_unit_id"]
    }
    if (value === undefined) annotationFailure("owner")
    if (value === null) return null
    return stringValue(value)
  }
}
