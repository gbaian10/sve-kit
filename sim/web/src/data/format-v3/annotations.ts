import {
  arrayValue,
  canonicalText,
  compareCodePoints,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
  utf8,
} from "./json"
import type { View } from "./reader"
import { digest } from "./sha256"
import { annotationFailure, type TextOwner, TextOwners } from "./text-owners"

type Row = JsonObject

function index(rows: readonly Row[], fields: readonly string[]): Map<string, Row> {
  const result = new Map<string, Row>()
  for (const row of rows) {
    const key = canonicalText(fields.map((field) => row[field] ?? null))
    if (result.has(key)) annotationFailure("duplicate_key")
    result.set(key, row)
  }
  return result
}

function get(rows: ReadonlyMap<string, Row>, key: readonly JsonValue[]): Row {
  const value = rows.get(canonicalText([...key]))
  if (!value) annotationFailure("reference")
  return value
}

function ordered(values: readonly JsonValue[]): void {
  const keys = values.map((value) => canonicalText(value))
  if (keys.some((key, i) => i > 0 && compareCodePoints(keys[i - 1] ?? "", key) >= 0))
    annotationFailure("ordering")
}

function ranges(text: string, occurrences: readonly Row[]): void {
  const all: [number, number][] = []
  const order: [number, number, string][] = []
  for (const [ordinal, occurrence] of occurrences.entries()) {
    if (occurrence["ordinal"] !== ordinal) annotationFailure("ordering")
    const spans: [number, number][] = arrayValue(occurrence["ranges"]).map((value) => {
      const span = objectValue(value)
      const start = span["start"],
        end = span["end"]
      if (
        typeof start !== "number" ||
        typeof end !== "number" ||
        !Number.isSafeInteger(start) ||
        !Number.isSafeInteger(end) ||
        start < 0 ||
        start >= end ||
        end > Array.from(text).length
      )
        annotationFailure("range")
      return [start, end]
    })
    if (!spans.length || spans.some((span, i) => i > 0 && (spans[i - 1]?.[1] ?? 0) > span[0]))
      annotationFailure("range")
    all.push(...spans)
    order.push([
      spans[0]?.[0] ?? 0,
      spans.at(-1)?.[1] ?? 0,
      canonicalText(occurrence["reference"] ?? null),
    ])
  }
  if (
    order.some((value, i) => {
      const previous = order[i - 1]
      return (
        previous !== undefined &&
        (previous[0] > value[0] ||
          (previous[0] === value[0] &&
            (previous[1] > value[1] ||
              (previous[1] === value[1] && compareCodePoints(previous[2], value[2]) > 0))))
      )
    })
  )
    annotationFailure("ordering")
  all.sort((a, b) => a[0] - b[0] || a[1] - b[1])
  if (all.some((value, i) => i > 0 && (all[i - 1]?.[1] ?? 0) > value[0])) annotationFailure("range")
}

class Closure {
  readonly owners: TextOwners
  readonly texts: Map<string, Row>
  readonly sets: Map<string, Row>
  readonly translations: Map<string, Row>
  readonly fields: Map<string, Row>
  readonly concepts: Map<string, Row>
  readonly vocabulary: Map<string, Row>
  readonly usedSets = new Set<string>()
  readonly usedTranslations = new Set<string>()
  readonly usedConcepts = new Set<string>()
  readonly bold = new Map<string, JsonValue>()

  readonly view: View
  readonly languages: readonly string[]

  constructor(view: View, languages: readonly string[]) {
    this.view = view
    this.languages = languages
    this.owners = new TextOwners(view)
    this.texts = index(view["text_unit"] ?? [], ["id"])
    this.sets = index(view["annotation_set"] ?? [], ["id"])
    this.translations = index(view["translation"] ?? [], ["id"])
    this.fields = index(view["field_annotation"] ?? [], ["owner", "field", "ordinal"])
    this.concepts = index(view["annotation_concept"] ?? [], ["id"])
    this.vocabulary = index(view["vocabulary"] ?? [], ["kind", "code"])
  }

  text(id: JsonValue | undefined): Row {
    return get(this.texts, [id ?? null])
  }

  pointer(value: Row): string {
    try {
      return this.owners.pointer(value)
    } catch {
      return annotationFailure("owner")
    }
  }

  owner(value: Row): TextOwner {
    try {
      return this.owners.get(value)
    } catch {
      return annotationFailure("owner")
    }
  }

  originalSet(pointer: Row): JsonValue {
    return (
      this.fields.get(
        canonicalText([
          pointer["owner"] ?? null,
          pointer["field"] ?? null,
          pointer["ordinal"] ?? null,
        ]),
      )?.["annotation_set_id"] ?? null
    )
  }

  setForText(id: JsonValue | undefined, unit: JsonValue | undefined): void {
    if (id === null) return
    const row = get(this.sets, [id ?? null])
    if (row["text_unit_id"] !== unit) annotationFailure("text_identity")
    this.usedSets.add(canonicalText([id ?? null]))
  }
}

function concepts(closure: Closure): void {
  const cards = new Set((closure.view["card"] ?? []).map((row) => row["id"]))
  const targets = new Map(
    ["keyword", "cr_clause", "ruling_revision"].flatMap((table) =>
      (closure.view[table] ?? []).map(
        (row) => [canonicalText([table, row["id"] ?? null]), row] as const,
      ),
    ),
  )
  const fields: Readonly<Record<string, string>> = {
    keyword: "definition_unit_id",
    cr_clause: "text_unit_id",
    ruling_revision: "decision_unit_id",
  }
  for (const row of closure.concepts.values()) {
    ordered(arrayValue(row["card_ids"]))
    ordered(arrayValue(row["explanations"]))
    if (
      arrayValue(row["card_ids"]).some((id) => !cards.has(id)) ||
      (row["category"] !== "card_name" && arrayValue(row["card_ids"]).length)
    )
      annotationFailure("reference")
    for (const raw of arrayValue(row["explanations"])) {
      const ref = objectValue(raw)
      const target = get(targets, [ref["kind"] ?? null, ref["id"] ?? null])
      const unit = target[fields[stringValue(ref["kind"])] ?? ""]
      if (unit === null || unit === undefined) annotationFailure("reference")
      closure.text(unit)
    }
  }
}

function reference(closure: Closure, occurrence: Row): void {
  const ref = objectValue(occurrence["reference"])
  let fixed: boolean
  if (ref["kind"] === "vocabulary") {
    const key = arrayValue(ref["key"])
    get(closure.vocabulary, key)
    fixed = key[0] === "class" || key[0] === "type"
  } else {
    const id = ref[ref["kind"] === "card_name" ? "term_id" : "key"]
    const concept = get(closure.concepts, [id ?? null])
    if ((ref["kind"] === "card_name") !== (concept["category"] === "card_name"))
      annotationFailure("reference")
    closure.usedConcepts.add(canonicalText([id ?? null]))
    fixed = concept["category"] !== "rule_term"
  }
  if (fixed && occurrence["bold"] !== true) annotationFailure("emphasis")
  const key = canonicalText(ref)
  if (closure.bold.has(key) && closure.bold.get(key) !== occurrence["bold"])
    annotationFailure("emphasis")
  closure.bold.set(key, occurrence["bold"] ?? null)
}

function sets(closure: Closure): void {
  for (const text of closure.texts.values()) {
    if (!closure.languages.includes(stringValue(text["lang"]))) annotationFailure("reference")
    if (
      text["id"] !==
      `t:${stringValue(text["lang"])}:${digest(utf8(stringValue(text["text"]))).slice(7, 23)}`
    )
      annotationFailure("identity")
  }
  for (const row of closure.sets.values()) {
    const text = stringValue(closure.text(row["text_unit_id"])["text"])
    const occurrences = arrayValue(row["occurrences"]).map((value) => objectValue(value))
    if (!occurrences.length) annotationFailure("range")
    ranges(text, occurrences)
    for (const occurrence of occurrences) reference(closure, occurrence)
    if (
      row["id"] !==
      `ann:${digest(utf8(canonicalText({ recipe: "annotation-v1", text_unit_id: row["text_unit_id"] ?? null, occurrences }))).slice(7)}`
    )
      annotationFailure("identity")
  }
  for (const row of closure.fields.values()) {
    const unit = closure.pointer({
      owner: row["owner"] ?? null,
      field: row["field"] ?? null,
      ordinal: row["ordinal"] ?? null,
    })
    closure.text(unit)
    closure.setForText(row["annotation_set_id"], unit)
  }
}

function sameFace(left: TextOwner, right: TextOwner): void {
  if (
    left.owner["kind"] !== right.owner["kind"] ||
    !left.cardId ||
    left.cardId !== right.cardId ||
    left.faceId !== right.faceId
  )
    annotationFailure("owner")
}

function jp(closure: Closure, receiver: TextOwner, value: Row, translation: Row): void {
  const pointer = objectValue(value["source"])
  const source = closure.owner(objectValue(pointer["owner"]))
  sameFace(receiver, source)
  if (
    receiver.region !== "en" ||
    source.region !== "jp" ||
    receiver.mappingState !== "confirmed" ||
    source.mappingState !== "confirmed"
  )
    annotationFailure("basis")
  if (
    !["name", "effect", "flavor"].includes(stringValue(value["field"])) ||
    value["ordinal"] !== null ||
    value["target_lang"] !== "zh-Hant" ||
    value["counterpart"] !== null
  )
    annotationFailure("basis")
  if (pointer["field"] !== value["field"] || pointer["ordinal"] !== null) annotationFailure("owner")
  const allowed =
    translation["authority"] === "unofficial" &&
    ["project", "machine"].includes(stringValue(translation["origin"]))
  const nameOfficial =
    value["field"] === "name" &&
    translation["authority"] === "digital_official" &&
    translation["origin"] === "official"
  if (!allowed && !nameOfficial) annotationFailure("basis")
}

function counterpart(closure: Closure, receiver: TextOwner, value: Row, translation: Row): void {
  if (
    value["counterpart"] === null ||
    translation["origin"] !== "official" ||
    translation["authority"] !== "sve_official" ||
    !["ja", "en"].includes(stringValue(value["target_lang"]))
  )
    annotationFailure("basis")
  const pointer = objectValue(value["counterpart"])
  const unit = closure.pointer(pointer)
  const donor = closure.owner(objectValue(pointer["owner"]))
  sameFace(receiver, donor)
  if (
    receiver.region === donor.region ||
    !["jp", "en"].includes(donor.region ?? "") ||
    !["jp", "en"].includes(receiver.region ?? "")
  )
    annotationFailure("basis")
  if (pointer["field"] !== value["field"]) annotationFailure("owner")
  if (
    closure.text(unit)["lang"] !== value["target_lang"] ||
    unit !== translation["text_unit_id"] ||
    closure.originalSet(pointer) !== translation["annotation_set_id"]
  )
    annotationFailure("text_identity")
}

function selection(closure: Closure, receiver: TextOwner, value: Row): void {
  const translation = get(closure.translations, [value["translation_id"] ?? null])
  closure.usedTranslations.add(canonicalText([value["translation_id"] ?? null]))
  const source = objectValue(value["source"])
  const pointer: Row = {
    owner: receiver.owner,
    field: value["field"] ?? null,
    ordinal: value["ordinal"] ?? null,
  }
  const basis = value["basis"]
  if (basis === "jp_source") jp(closure, receiver, value, translation)
  else if (basis === "own_source" || basis === "official_counterpart") {
    if (basis === "official_counterpart") counterpart(closure, receiver, value, translation)
    if (canonicalText(source) !== canonicalText(pointer)) annotationFailure("owner")
    if (basis === "own_source" && value["counterpart"] !== null) annotationFailure("basis")
  } else annotationFailure("basis")
  closure.pointer(pointer)
  if (closure.pointer(source) !== translation["source_unit_id"]) annotationFailure("text_identity")
  if (basis === "jp_source" && closure.text(translation["source_unit_id"])["lang"] !== "ja")
    annotationFailure("basis")
  if (
    value["target_lang"] !== translation["target_lang"] ||
    closure.text(translation["text_unit_id"])["lang"] !== value["target_lang"]
  )
    annotationFailure("text_identity")
  closure.setForText(translation["annotation_set_id"], translation["text_unit_id"])
}

export function validateAnnotations(view: View, languages: readonly string[]): void {
  const closure = new Closure(view, languages)
  concepts(closure)
  sets(closure)
  for (const owner of closure.owners.byOwner.values()) {
    const values = arrayValue(owner.row["translations"]).map((value) => objectValue(value))
    index(values, ["field", "ordinal", "target_lang"])
    for (const value of values) selection(closure, owner, value)
  }
  for (const [used, rows] of [
    [closure.usedSets, closure.sets],
    [closure.usedTranslations, closure.translations],
    [closure.usedConcepts, closure.concepts],
  ] as const) {
    if (used.size !== rows.size || [...rows.keys()].some((key) => !used.has(key)))
      annotationFailure("reference")
  }
}
