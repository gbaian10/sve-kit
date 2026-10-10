import casesJson from "../../../../docs/schema/export/public-annotation-cases.json"
import { createSnapshotClient, type LoadedSnapshot, type SnapshotClient } from "./client"
import { decodeRow } from "./format-v3/decode"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
  utf8,
} from "./format-v3/json"
import type { Fragment } from "./format-v3/reader"
import type { View } from "./format-v3/reader"
import { digest } from "./format-v3/sha256"

export const publicCases = objectValue(casesJson)

function parent(value: JsonValue, pointer: string): [JsonValue, string] {
  const parts = pointer
    .split("/")
    .slice(1)
    .map((part) => part.replace(/~1/g, "/").replace(/~0/g, "~"))
  let current = value
  for (const part of parts.slice(0, -1))
    current = Array.isArray(current)
      ? (current[Number(part)] ?? null)
      : (objectValue(current)[part] ?? null)
  return [current, parts.at(-1) ?? ""]
}

function mutate(value: JsonValue, parameters: JsonObject): JsonValue {
  let result = structuredClone(value)
  for (const [pointer, replacement] of Object.entries(objectValue(parameters["changes"] ?? {}))) {
    if (pointer === "") {
      result = structuredClone(replacement)
      continue
    }
    const [current, last] = parent(result, pointer)
    if (Array.isArray(current)) current[Number(last)] = structuredClone(replacement)
    else objectValue(current)[last] = structuredClone(replacement)
  }
  for (const pointer of arrayValue(parameters["remove"] ?? [])) {
    const [current, last] = parent(result, stringValue(pointer))
    if (Array.isArray(current)) current.splice(Number(last), 1)
    else Reflect.deleteProperty(objectValue(current), last)
  }
  return result
}

function usedSets(value: JsonObject): Set<JsonValue> {
  return new Set([
    ...arrayValue(value["field_annotations"]).map((row) => arrayValue(row)[3] ?? null),
    ...arrayValue(value["translations"])
      .map((row) => arrayValue(row)[7] ?? null)
      .filter((id) => id !== null),
  ])
}
function usedConcepts(value: JsonObject): Set<JsonValue> {
  return new Set(
    arrayValue(value["annotation_sets"]).flatMap((row) =>
      arrayValue(arrayValue(row)[2]).flatMap((occurrence) => {
        const reference = objectValue(arrayValue(occurrence)[1])
        return reference["kind"] === "vocabulary"
          ? []
          : [reference[reference["kind"] === "card_name" ? "term_id" : "key"] ?? null]
      }),
    ),
  )
}
function expand(before: JsonValue, parameters: JsonObject): JsonValue {
  const after = mutate(before, parameters)
  if (
    !Array.isArray(before) &&
    before !== null &&
    typeof before === "object" &&
    "annotation_sets" in before
  ) {
    const value = objectValue(after)
    const oldSets = usedSets(before),
      currentSets = usedSets(value)
    value["annotation_sets"] = arrayValue(value["annotation_sets"]).filter(
      (row) =>
        !oldSets.has(arrayValue(row)[0] ?? null) || currentSets.has(arrayValue(row)[0] ?? null),
    )
    const oldConcepts = usedConcepts(before),
      currentConcepts = usedConcepts(value)
    value["concepts"] = arrayValue(value["concepts"]).filter(
      (row) =>
        !oldConcepts.has(arrayValue(row)[0] ?? null) ||
        currentConcepts.has(arrayValue(row)[0] ?? null),
    )
  }
  return after
}
function fixture(name: string): JsonValue {
  const value = structuredClone(objectValue(publicCases["fixtures"])[name] ?? null)
  if (value !== null && !Array.isArray(value) && typeof value === "object" && "fixture" in value)
    return expand(fixture(stringValue(value["fixture"])), value)
  return value
}
export function annotationInputs(caseValue: JsonObject): JsonValue[] {
  const parameters = objectValue(caseValue["input"])
  const base =
    parameters["fixture"] === undefined ? parameters : fixture(stringValue(parameters["fixture"]))
  const result = expand(base, parameters)
  return arrayValue(parameters["variants"] ?? [{}]).map((variant) =>
    expand(result, objectValue(variant)),
  )
}

function ownerRow(owner: JsonObject, fields: JsonValue): JsonObject {
  const labelField = "label"
  const names: Record<string, Record<string, string>> = {
    face_revision: { name: "name_unit_id", effect: "effect_unit_id" },
    printing_face: {
      name: "printed_name_unit_id",
      effect: "printed_effect_unit_id",
      flavor: "flavor_unit_id",
    },
    qa_version: { question: "question_unit_id", answer: "answer_unit_id" },
    cr_clause: { effect: "text_unit_id" },
    vocabulary: { [labelField]: "label_unit_id" },
    product: { [labelField]: "name_unit_id" },
    product_family: { [labelField]: "name_unit_id" },
    keyword: { [labelField]: "name_unit_id", effect: "definition_unit_id" },
  }
  const columns = names[stringValue(owner["kind"])]
  if (!columns) throw new Error("unknown fixture owner")
  const result: JsonObject = {
    ...Object.fromEntries(Object.values(columns).map((column) => [column, null])),
    translations: [],
    sections: [],
    actions: [],
  }
  for (const raw of arrayValue(fields)) {
    const [name, ordinal, unit] = arrayValue(raw)
    if (name === "section")
      arrayValue(result["sections"]).push({ ordinal: ordinal ?? null, text_unit_id: unit ?? null })
    else if (name === "action_label")
      arrayValue(result["actions"]).push({ label_unit_id: unit ?? null })
    else result[columns[stringValue(name)] ?? ""] = unit ?? null
  }
  return result
}

export function annotationView(value: JsonObject): View {
  const result: View = Object.fromEntries(
    [
      "card",
      "face",
      "face_revision",
      "printing",
      "qa_version",
      "cr_clause",
      "product",
      "product_family",
      "keyword",
      "vocabulary",
      "ruling_revision",
      "text_unit",
      "annotation_set",
      "field_annotation",
      "translation",
      "annotation_concept",
    ].map((table) => [table, []]),
  )
  result["text_unit"] = arrayValue(value["text_units"]).map((row) => {
    const [id, lang, text] = arrayValue(row)
    return { id: id ?? null, lang: lang ?? null, text: text ?? null }
  })
  for (const [table, column] of [
    ["annotation_set", "annotation_sets"],
    ["field_annotation", "field_annotations"],
    ["translation", "translations"],
    ["annotation_concept", "concepts"],
  ])
    result[table ?? ""] = arrayValue(value[column ?? ""]).map((row) => decodeRow(table ?? "", row))
  result["vocabulary"] = arrayValue(value["vocabulary"]).map((row) => {
    const [kind, code, label_unit_id, active, translations] = arrayValue(row)
    return {
      kind: kind ?? null,
      code: code ?? null,
      label_unit_id: label_unit_id ?? null,
      active: active ?? null,
      translations: translations ?? [],
    }
  })
  const cards = new Map(
    arrayValue(value["cards"]).map((id) => [stringValue(id), { id, regions: [] }]),
  )
  result["card"] = [...cards.values()]
  const owners = new Map<string, JsonObject>(),
    translated = new Map<string, JsonObject>(),
    faces = new Map<string, JsonObject>()
  for (const raw of arrayValue(value["owners"])) {
    const info = objectValue(raw),
      owner = objectValue(info["owner"]),
      kind = stringValue(owner["kind"])
    let row = ownerRow(owner, info["fields"] ?? [])
    const publicOwner = structuredClone(owner)
    if (["face_revision", "printing_face"].includes(kind)) {
      const cardId = stringValue(info["card_id"]),
        region = stringValue(info["region"])
      const faceId = `f:fixture:${digest(utf8(canonicalText([info["card_id"] ?? null, info["face_id"] ?? null]))).slice(7, 23)}`
      faces.set(faceId, { id: faceId, card_id: cardId })
      const card = cards.get(cardId)
      if (!card) throw new Error("missing fixture card")
      const regions = arrayValue(card["regions"])
      if (!regions.some((entry) => objectValue(entry)["region"] === region))
        regions.push({ region, mapping_state: info["mapping_state"] ?? null })
      row["face_id"] = faceId
      if (kind === "face_revision") {
        row["id"] = owner["id"] ?? null
        row["region"] = region
        result[kind]?.push(row)
      } else {
        publicOwner["face_id"] = faceId
        result["printing"]?.push({ id: owner["id"] ?? null, card_id: cardId, region, faces: [row] })
      }
    } else if (kind === "vocabulary") {
      const found = result[kind]?.find(
        (row) => row["kind"] === owner["vocabulary_kind"] && row["code"] === owner["code"],
      )
      if (!found) continue
      row = found
    } else {
      row["id"] = owner["id"] ?? null
      result[kind]?.push(row)
    }
    owners.set(canonicalText(owner), row)
    translated.set(canonicalText(owner), publicOwner)
  }
  result["face"] = [...faces.values()]
  for (const row of result["field_annotation"] ?? [])
    row["owner"] = translated.get(canonicalText(row["owner"] ?? null)) ?? row["owner"] ?? null
  for (const raw of arrayValue(value["field_translations"])) {
    const entry = objectValue(raw),
      receiver = decodeRow("PublicTextPointer", entry["receiver"] ?? null),
      selection = decodeRow("FieldTranslation", entry["value"] ?? null)
    for (const name of ["source", "counterpart"])
      if (selection[name] !== null) {
        const pointer = objectValue(selection[name])
        pointer["owner"] =
          translated.get(canonicalText(pointer["owner"] ?? null)) ?? pointer["owner"] ?? null
      }
    const row = owners.get(canonicalText(receiver["owner"] ?? null))
    if (!row) throw new Error("missing fixture receiver")
    arrayValue(row["translations"]).push(selection)
  }
  const explanationColumns: Record<string, string> = {
    keyword: "definition_unit_id",
    cr_clause: "text_unit_id",
    ruling_revision: "decision_unit_id",
  }
  for (const raw of arrayValue(value["explanation_targets"] ?? [])) {
    const target = objectValue(raw),
      kind = stringValue(target["kind"])
    result[kind]?.push({
      id: target["id"] ?? null,
      [explanationColumns[kind] ?? ""]: target["text_unit_id"] ?? null,
      translations: [],
    })
  }
  return result
}

export function componentInput(value: JsonObject): JsonObject {
  const unit = value["text_unit"] ??
    arrayValue(value["text_units"] ?? [])[0] ?? ["t:ja:559aead08264d579", "ja", "A"]
  if (!unit) throw new Error("component requires exact text")
  let sets = arrayValue(value["sets"] ?? [])
  if (value["annotation_set"] !== undefined && value["annotation_set"] !== null)
    sets = [value["annotation_set"]]
  const references =
    value["reference"] === undefined ? arrayValue(value["references"] ?? []) : [value["reference"]]
  if (references.length)
    sets = references.map((reference) => {
      const occurrences: JsonValue[] = [
        [0, reference, [[0, 1]], value["bold"] ?? ("bold" in value ? null : true)],
      ]
      const id = `ann:${digest(utf8(canonicalText({ recipe: "annotation-v1", text_unit_id: arrayValue(unit)[0] ?? null, occurrences: occurrences.map((row) => decodeRow("Annotation", row)) }))).slice(7)}`
      return [id, arrayValue(unit)[0] ?? null, occurrences]
    })
  let uses = arrayValue(value["uses"] ?? [])
  if (value["pointer"] !== undefined)
    uses = [[...arrayValue(value["pointer"]), arrayValue(sets[0])[0] ?? null]]
  else if (!uses.length)
    uses = sets.map((row, index) => [
      { kind: "qa_version", id: `qa:component:${String(index)}` },
      "question",
      null,
      arrayValue(row)[0] ?? null,
    ])
  let concepts = arrayValue(value["concepts"] ?? [])
  const vocabulary: JsonValue[] = []
  const inferred = new Map<string, JsonValue>()
  for (const row of sets)
    for (const occurrence of arrayValue(arrayValue(row)[2])) {
      const reference = objectValue(arrayValue(occurrence)[1])
      if (reference["kind"] === "vocabulary")
        vocabulary.push([...arrayValue(reference["key"]), arrayValue(unit)[0] ?? null, true, []])
      else {
        const id = stringValue(reference[reference["kind"] === "card_name" ? "term_id" : "key"])
        inferred.set(id, [
          id,
          value["category"] ?? (reference["kind"] === "card_name" ? "card_name" : "rule_term"),
          [],
          [],
        ])
      }
    }
  if (value["concepts"] === undefined) concepts = [...inferred.values()]
  if (value["owner"] !== undefined) {
    const owner = objectValue(value["owner"])
    if (owner["kind"] === "vocabulary")
      vocabulary.push([
        owner["vocabulary_kind"] ?? null,
        owner["code"] ?? null,
        arrayValue(unit)[0] ?? null,
        true,
        [],
      ])
  }
  const owners = uses.map((use) => {
    const [owner, field, ordinal, annotationId] = arrayValue(use)
    const set = sets.find((row) => arrayValue(row)[0] === annotationId)
    if (!set) throw new Error("missing component set")
    return {
      owner: owner ?? null,
      fields: value["fields"] ?? [[field ?? null, ordinal ?? null, arrayValue(set)[1] ?? null]],
      card_id: "c:component",
      face_id: "f:component",
      region: "jp",
      mapping_state: "confirmed",
    }
  })
  return {
    languages: ["en", "ja", "zh-Hant"],
    text_units: value["text_units"] ?? [unit],
    annotation_sets: sets,
    field_annotations: uses,
    translations: [],
    field_translations: [],
    concepts,
    vocabulary,
    owners,
    cards: value["cards"] ?? ["c:component"],
    explanation_targets: value["explanation_targets"] ?? [],
  }
}

export function displayClient(value: JsonObject): SnapshotClient {
  const view = annotationView(value)
  for (const card of view["card"] ?? []) card["home_set_id"] = "home:component"
  for (const face of view["face"] ?? []) {
    face["current"] = (view["face_revision"] ?? [])
      .filter((revision) => revision["face_id"] === face["id"])
      .map((revision) => ({
        region: revision["region"] ?? null,
        revision_id: revision["id"] ?? null,
      }))
    face["wording"] = []
  }
  const fragments: Fragment[] = []
  for (const [table, rows] of Object.entries(view)) {
    if (table === "field_annotation") {
      for (const row of rows) {
        const owner = objectValue(row["owner"])
        const physical = ["face_revision", "printing_face"].includes(stringValue(owner["kind"]))
          ? { kind: "home_set", id: "home:component" }
          : { kind: "global", id: null }
        const partition =
          owner["kind"] === "face_revision" && row["field"] === "name" ? "bootstrap" : "detail"
        fragments.push({
          identity: canonicalText([table, row["owner"] ?? null]),
          file: `field:${canonicalText(row["owner"] ?? null)}`,
          table,
          rows: [row],
          value: { owner: physical, partition, base: null },
        })
      }
    } else
      fragments.push({
        identity: table,
        file: table,
        table,
        rows,
        value: { owner: { kind: "global", id: null }, partition: "bootstrap", base: null },
      })
  }
  const snapshot: LoadedSnapshot = {
    dataVersion: "synthetic",
    manifestHash: "synthetic",
    manifest: { partitioning: { bucket_count: 64 } },
    config: { languages: arrayValue(value["languages"]).map((code) => ({ code })) },
    files: new Map(),
    bootstrap: fragments,
  }
  // Logical display vectors start after byte admission; network and wire vectors are separate.
  const client = createSnapshotClient("https://example.invalid", {
    fetch: () => Promise.reject(new Error("unexpected fixture fetch")),
  })
  return {
    ...client,
    snapshot: () => snapshot,
    fragments: () => Promise.reject(new Error("unexpected logical fragment request")),
  }
}
