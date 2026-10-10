import type { LoadedSnapshot, SnapshotClient } from "./client"
import { validateAnnotations } from "./format-v3/annotations"
import {
  arrayValue,
  canonicalText,
  integerValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./format-v3/json"
import type { Fragment, View } from "./format-v3/reader"
import { definition } from "./format-v3/schema"
import { annotationFailure, TextOwners } from "./format-v3/text-owners"
import { bucketOf, createLocator, GLOBAL_OWNER } from "./locator"

export interface AnnotatedText {
  readonly unit: JsonObject
  readonly annotation: JsonObject | null
}

export interface SelectedText {
  readonly printedState?: string
  readonly original: AnnotatedText
  readonly translated?: AnnotatedText
  readonly source?: AnnotatedText
  readonly selection?: JsonObject
  readonly translation?: JsonObject
  readonly concepts: readonly JsonObject[]
  readonly explanations: readonly {
    readonly reference: JsonObject
    readonly row: JsonObject
    readonly unit: JsonObject
  }[]
  readonly cards: readonly {
    readonly cardId: string
    readonly intId: number
    readonly cardNo: string
  }[]
}

export interface AnnotatedTextResolver {
  /** Resolves only after the exact owner, ranges and reference closure have passed R. */
  readonly resolve: (pointer: JsonObject, targetLang: string) => Promise<SelectedText | null>
}

const OWNER_TABLES = [
  "face_revision",
  "qa_version",
  "cr_clause",
  "vocabulary",
  "product_family",
  "product",
  "keyword",
]

function key(table: string, row: JsonObject): string {
  return canonicalText(
    arrayValue(definition(table)["x-primary-key"]).map((field) => row[stringValue(field)] ?? null),
  )
}

class PageClosure {
  readonly rows = new Map<string, Map<string, JsonObject>>()
  readonly placements = new Map<string, Fragment>()
  readonly loaded = new Set<string>()
  readonly snapshot: LoadedSnapshot
  readonly client: SnapshotClient
  readonly locate: ReturnType<typeof createLocator>
  readonly count: number

  constructor(client: SnapshotClient, snapshot: LoadedSnapshot) {
    this.client = client
    this.snapshot = snapshot
    this.locate = createLocator(snapshot.files)
    this.count = integerValue(objectValue(snapshot.manifest["partitioning"])["bucket_count"])
    this.add(snapshot.bootstrap)
    for (const fragment of snapshot.bootstrap) this.loaded.add(fragment.file)
  }

  add(fragments: readonly Fragment[]): void {
    for (const fragment of fragments) {
      const rows = this.rows.get(fragment.table) ?? new Map<string, JsonObject>()
      for (const row of fragment.rows) {
        const identity = key(fragment.table, row)
        rows.set(identity, row)
        this.placements.set(canonicalText([fragment.table, identity]), fragment)
      }
      this.rows.set(fragment.table, rows)
    }
  }

  view(): View {
    return Object.fromEntries([...this.rows].map(([table, rows]) => [table, [...rows.values()]]))
  }

  async load(
    table: string,
    primaryKey: readonly JsonValue[],
    partition: "bootstrap" | "detail" | "history",
    physicalOwner = GLOBAL_OWNER,
    entityKey = primaryKey,
  ): Promise<void> {
    const file = this.locate({
      table,
      owner: physicalOwner,
      bucket: bucketOf(entityKey, this.count),
      partition,
    })
    // A missing logical fragment is empty only after consulting this verified manifest.
    if (file === undefined || this.loaded.has(file)) return
    this.add(await this.client.fragments(file))
    if (this.client.snapshot() !== this.snapshot) throw new Error("snapshot replaced")
    this.loaded.add(file)
  }

  async row(table: string, id: JsonValue): Promise<JsonObject> {
    await this.load(table, [id], "detail")
    const row = this.rows.get(table)?.get(canonicalText([id]))
    if (!row) annotationFailure("reference")
    return row
  }

  owner(value: JsonObject) {
    return new TextOwners(this.view()).get(value)
  }

  location(value: JsonObject, field: string) {
    const owner = this.owner(value)
    const kind = stringValue(value["kind"])
    if (kind === "face_revision") {
      const card = this.rows.get("card")?.get(canonicalText([owner.cardId ?? null]))
      if (!card) annotationFailure("owner")
      const displayed = this.snapshot.bootstrap.some(
        (fragment) =>
          fragment.table === kind && fragment.rows.some((row) => row["id"] === value["id"]),
      )
      return {
        physical: { kind: "home_set", id: card["home_set_id"] ?? null },
        entity: [owner.cardId ?? null],
        partition: displayed ? (field === "name" ? "bootstrap" : "detail") : "history",
      } as const
    }
    if (kind === "printing_face") {
      const fragment = this.snapshot.bootstrap.find(
        (fragment) =>
          fragment.table === "printing" && fragment.rows.some((row) => row["id"] === value["id"]),
      )
      if (!fragment) annotationFailure("owner")
      return {
        physical: objectValue(fragment.value["owner"]),
        entity: [value["id"] ?? null],
        partition: "detail",
      } as const
    }
    return {
      physical: GLOBAL_OWNER,
      entity: [value, field, null],
      partition: ["keyword", "vocabulary", "product", "product_family"].includes(kind)
        ? "bootstrap"
        : "detail",
    } as const
  }

  async original(pointer: JsonObject): Promise<{ unitId: string; annotationId: JsonValue } | null> {
    const value = objectValue(pointer["owner"])
    const field = stringValue(pointer["field"])
    const kind = stringValue(value["kind"])
    if (!OWNER_TABLES.includes(kind) && kind !== "printing_face") annotationFailure("owner")
    if (!["face_revision", "printing_face"].includes(kind)) {
      const pk =
        kind === "vocabulary"
          ? [value["vocabulary_kind"] ?? null, value["code"] ?? null]
          : [value["id"] ?? null]
      await this.load(kind, pk, "detail")
    }
    const location = this.location(value, field)
    if (
      kind === "printing_face" ||
      (kind === "face_revision" && location.partition !== "bootstrap")
    )
      await this.load(
        kind === "printing_face" ? "printing" : kind,
        [value["id"] ?? null],
        location.partition === "bootstrap" ? "detail" : location.partition,
        location.physical,
        location.entity,
      )
    const pk = [value, pointer["field"] ?? null, pointer["ordinal"] ?? null]
    await this.load(
      "field_annotation",
      pk,
      location.partition,
      location.physical,
      ["face_revision", "printing_face"].includes(kind) ? location.entity : pk,
    )
    const unitId = new TextOwners(this.view()).optionalPointer(pointer)
    if (unitId === null) {
      if (
        this.rows.get("field_annotation")?.has(canonicalText(pk)) ||
        arrayValue(this.owner(value).row["translations"]).some((raw) => {
          const selection = objectValue(raw)
          return selection["field"] === field && selection["ordinal"] === pointer["ordinal"]
        })
      )
        annotationFailure("owner")
      return null
    }
    await this.row("text_unit", unitId)
    const annotationRow = this.rows.get("field_annotation")?.get(canonicalText(pk))
    if (annotationRow) {
      const physical = this.placements.get(canonicalText(["field_annotation", canonicalText(pk)]))
      if (
        !physical ||
        physical.value["partition"] !== location.partition ||
        canonicalText(physical.value["owner"] ?? null) !== canonicalText(location.physical)
      )
        annotationFailure("owner")
    }
    const annotationId = annotationRow?.["annotation_set_id"] ?? null
    return { unitId, annotationId }
  }
}

export function createAnnotatedTextResolver(client: SnapshotClient): AnnotatedTextResolver {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  return {
    resolve: async (pointer, targetLang) => {
      const page = new PageClosure(client, snapshot)
      const original = await page.original(pointer)
      if (original === null) return null
      const receiver = page.owner(objectValue(pointer["owner"]))
      const selected = arrayValue(receiver.row["translations"])
        .map((value) => objectValue(value))
        .filter(
          (value) =>
            value["field"] === pointer["field"] &&
            value["ordinal"] === pointer["ordinal"] &&
            value["target_lang"] === targetLang,
        )
      if (selected.length > 1) annotationFailure("duplicate_key")
      const selection = selected[0]
      const translation = selection
        ? await page.row("translation", selection["translation_id"] ?? null)
        : undefined
      const sourcePointer = selection ? objectValue(selection["source"]) : pointer
      const source = await page.original(sourcePointer)
      if (source === null) annotationFailure("owner")
      const pointers = [pointer, sourcePointer]
      if (selection?.["counterpart"] !== null && selection?.["counterpart"] !== undefined) {
        const counterpart = objectValue(selection["counterpart"])
        await page.original(counterpart)
        pointers.push(counterpart)
      }
      const fieldRows = pointers.flatMap((value) => {
        const row = page.rows
          .get("field_annotation")
          ?.get(
            canonicalText([
              value["owner"] ?? null,
              value["field"] ?? null,
              value["ordinal"] ?? null,
            ]),
          )
        return row ? [row] : []
      })
      const ids = new Set(fieldRows.map((row) => row["annotation_set_id"]))
      if (
        translation?.["annotation_set_id"] !== null &&
        translation?.["annotation_set_id"] !== undefined
      )
        ids.add(translation["annotation_set_id"])
      const annotationRows = await Promise.all(
        [...ids].map((id) => page.row("annotation_set", id ?? null)),
      )
      const conceptIds = new Set<JsonValue>()
      for (const row of annotationRows) {
        await page.row("text_unit", row["text_unit_id"] ?? null)
        for (const raw of arrayValue(row["occurrences"])) {
          const ref = objectValue(objectValue(raw)["reference"])
          if (ref["kind"] !== "vocabulary")
            conceptIds.add(ref[ref["kind"] === "card_name" ? "term_id" : "key"] ?? null)
        }
      }
      const concepts = await Promise.all(
        [...conceptIds].map((id) => page.row("annotation_concept", id)),
      )
      const explanations = []
      for (const concept of concepts)
        for (const raw of arrayValue(concept["explanations"])) {
          const ref = objectValue(raw)
          const table = stringValue(ref["kind"])
          const row = await page.row(table, ref["id"] ?? null)
          const column =
            table === "keyword"
              ? "definition_unit_id"
              : table === "cr_clause"
                ? "text_unit_id"
                : "decision_unit_id"
          const unit = await page.row("text_unit", row[column] ?? null)
          explanations.push({ reference: ref, row, unit })
        }
      if (translation) await page.row("text_unit", translation["text_unit_id"] ?? null)
      const view = page.view()
      const trim = (row: JsonObject): JsonObject => ({ ...row, translations: [] })
      for (const table of OWNER_TABLES) view[table] = (view[table] ?? []).map(trim)
      view["printing"] = (view["printing"] ?? []).map((row) => ({
        ...row,
        faces: arrayValue(row["faces"]).map((value) => trim(objectValue(value))),
      }))
      const selectedOwner = new TextOwners(view).get(objectValue(pointer["owner"]))
      selectedOwner.row["translations"] = selection ? [selection] : []
      view["field_annotation"] = [
        ...new Map(fieldRows.map((row) => [key("field_annotation", row), row])).values(),
      ]
      view["annotation_set"] = annotationRows
      view["annotation_concept"] = concepts
      view["translation"] = translation ? [translation] : []
      validateAnnotations(
        view,
        arrayValue(snapshot.config["languages"]).map((value) =>
          stringValue(objectValue(value)["code"]),
        ),
      )
      const annotated = (unitId: JsonValue, id: JsonValue): AnnotatedText => {
        const unit = page.rows.get("text_unit")?.get(canonicalText([unitId]))
        if (!unit) annotationFailure("reference")
        return {
          unit,
          annotation: id === null ? null : (annotationRows.find((row) => row["id"] === id) ?? null),
        }
      }
      return {
        ...(objectValue(pointer["owner"])["kind"] === "printing_face"
          ? { printedState: stringValue(receiver.row["printed_text_state"]) }
          : {}),
        original: annotated(original.unitId, original.annotationId),
        ...(translation && selection
          ? {
              translated: annotated(
                translation["text_unit_id"] ?? null,
                translation["annotation_set_id"] ?? null,
              ),
              source: annotated(source.unitId, source.annotationId),
              selection,
              translation,
            }
          : {}),
        concepts,
        explanations,
        cards: [...(page.rows.get("card")?.values() ?? [])]
          .filter((card) =>
            concepts.some((concept) =>
              arrayValue(concept["card_ids"]).includes(card["id"] ?? null),
            ),
          )
          .flatMap((card) => {
            const defaults = arrayValue(card["regions"]).map(
              (value) => objectValue(value)["default_printing_id"],
            )
            return [...(page.rows.get("printing")?.values() ?? [])]
              .filter((printing) => defaults.includes(printing["id"]))
              .map((printing) => ({
                cardId: stringValue(card["id"]),
                intId: integerValue(printing["int_id"]),
                cardNo: stringValue(printing["card_no"]),
              }))
          }),
      }
    },
  }
}
