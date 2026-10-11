import type { LoadedSnapshot, SnapshotClient } from "./client"
import { annotationLocations } from "./format-v3/annotation-placement"
import {
  arrayValue,
  canonicalText,
  integerValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./format-v3/json"
import { translationNameConcept, wholeName } from "./format-v3/name-annotations"
import type { Fragment, View } from "./format-v3/reader"
import { definition } from "./format-v3/schema"
import { annotationFailure, type TextOwner, TextOwners } from "./format-v3/text-owners"
import { bucketOf, createLocator, GLOBAL_OWNER, homeSetOwner } from "./locator"

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
  readonly vocabulary?: readonly {
    readonly kind: string
    readonly code: string
    readonly unit: JsonObject
  }[]
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
  /** Resolves after the exact owner and its annotation references have been loaded. */
  readonly resolve: (pointer: JsonObject, targetLang: string) => Promise<SelectedText | null>
  /** A text this resolver already resolved; undefined until `resolve` has settled for it. */
  readonly peek: (pointer: JsonObject, targetLang: string) => SelectedText | null | undefined
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
  readonly loaded = new Set<string>()
  private readonly loading = new Map<string, Promise<void>>()
  readonly snapshot: LoadedSnapshot
  readonly client: SnapshotClient
  readonly locate: ReturnType<typeof createLocator>
  readonly count: number
  readonly fragments: Fragment[] = []
  private owners: TextOwners | undefined
  private locations: ReturnType<typeof annotationLocations> | undefined

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
      this.fragments.push(fragment)
      const rows = this.rows.get(fragment.table) ?? new Map<string, JsonObject>()
      let replaced = false
      for (const row of fragment.rows) {
        const identity = key(fragment.table, row)
        replaced ||= rows.has(identity)
        rows.set(identity, row)
      }
      this.rows.set(fragment.table, rows)
      // Each index changes only when a table it reads arrives. Rows new to the closure extend the
      // owner index in place; a replaced row, or rows it rejects, leave it to the full rebuild,
      // which applies the same checks and reports the failure where it always has.
      const placed = ["card", "face", "printing", "face_revision"].includes(fragment.table)
      if (placed || OWNER_TABLES.includes(fragment.table)) {
        if (replaced) this.owners = undefined
        else
          try {
            this.owners?.include({ [fragment.table]: fragment.rows })
          } catch {
            this.owners = undefined
          }
      }
      if (placed) this.locations = undefined
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
    // Texts resolved together share this closure; a second caller waits for the same file
    // instead of adding its rows twice.
    let running = this.loading.get(file)
    if (!running) {
      running = this.client
        .fragments(file)
        .then((fragments) => {
          if (this.client.snapshot() !== this.snapshot) throw new Error("snapshot replaced")
          this.add(fragments)
          this.loaded.add(file)
        })
        .finally(() => this.loading.delete(file))
      this.loading.set(file, running)
    }
    await running
  }

  async row(table: string, id: JsonValue): Promise<JsonObject> {
    const identity = canonicalText([id])
    if (!this.rows.get(table)?.has(identity)) await this.load(table, [id], "detail")
    const row = this.rows.get(table)?.get(identity)
    if (!row) annotationFailure("reference")
    return row
  }

  owner(value: JsonObject) {
    return this.textOwners().get(value)
  }

  textOwners(): TextOwners {
    this.owners ??= new TextOwners(this.view())
    return this.owners
  }

  location(value: JsonObject, field: string, ordinal: JsonValue = null) {
    this.owner(value)
    this.locations ??= annotationLocations(this.view(), this.fragments)
    return this.locations(value, field, ordinal)
  }

  virtualAnnotation(annotation: JsonObject): void {
    const sets = this.rows.get("annotation_set") ?? new Map<string, JsonObject>()
    const identity = canonicalText([annotation["id"] ?? null])
    sets.set(identity, annotation)
    this.rows.set("annotation_set", sets)
  }

  async original(
    pointer: JsonObject,
    receiver?: TextOwner,
  ): Promise<{ unitId: string; annotationId: JsonValue } | null> {
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
    if (
      kind === "face_revision" &&
      !this.rows.get(kind)?.has(canonicalText([value["id"] ?? null]))
    ) {
      const card = this.rows.get("card")?.get(canonicalText([receiver?.cardId ?? null]))
      if (!card) annotationFailure("owner")
      const physical = homeSetOwner(stringValue(card["home_set_id"]))
      const entity = [card["id"] ?? null]
      // Same-face source owners can only live in this card's fixed bootstrap or history fragment.
      for (const partition of ["bootstrap", "history"] as const) {
        await this.load(kind, [value["id"] ?? null], partition, physical, entity)
        if (this.rows.get(kind)?.has(canonicalText([value["id"] ?? null]))) break
      }
    }
    const location = this.location(value, field, pointer["ordinal"] ?? null)
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
    const unitId = this.textOwners().optionalPointer(pointer)
    if (unitId === null) return null
    await this.row("text_unit", unitId)
    const annotationRow = this.rows.get("field_annotation")?.get(canonicalText(pk))
    let annotationId = annotationRow?.["annotation_set_id"] ?? null
    const concept = this.owner(value).row["name_concept_id"]
    if (field === "name" && pointer["ordinal"] === null && concept != null) {
      const unit = await this.row("text_unit", unitId)
      const annotation = wholeName(unit, stringValue(concept))
      annotationId = annotation["id"] ?? null
      this.virtualAnnotation(annotation)
      const fields = this.rows.get("field_annotation") ?? new Map<string, JsonObject>()
      fields.set(canonicalText(pk), { ...pointer, annotation_set_id: annotationId })
      this.rows.set("field_annotation", fields)
    }
    return { unitId, annotationId }
  }
}

const shared = new WeakMap<LoadedSnapshot, AnnotatedTextResolver>()

/**
 * The resolver every text on screen uses for the client's loaded snapshot. Indexing the bootstrap
 * is the expensive step, so a table of previews or a reopened card page must not repeat it.
 */
export function sharedAnnotatedTextResolver(client: SnapshotClient): AnnotatedTextResolver {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  let resolver = shared.get(snapshot)
  if (!resolver) {
    resolver = createAnnotatedTextResolver(client)
    shared.set(snapshot, resolver)
  }
  return resolver
}

export function createAnnotatedTextResolver(client: SnapshotClient): AnnotatedTextResolver {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const page = new PageClosure(client, snapshot)
  const resolveText = async (
    pointer: JsonObject,
    targetLang: string,
  ): Promise<SelectedText | null> => {
    if (client.snapshot() !== snapshot) throw new Error("snapshot replaced")
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
    const selection = selected[0]
    const translation = selection
      ? { ...(await page.row("translation", selection["translation_id"] ?? null)) }
      : undefined
    const sourcePointer = selection ? objectValue(selection["source"]) : pointer
    const source = await page.original(sourcePointer, receiver)
    if (source === null) annotationFailure("owner")
    if (translation?.["annotation_kind"] === "whole_name" && selection) {
      const unit = await page.row("text_unit", translation["text_unit_id"] ?? null)
      const annotation = wholeName(unit, translationNameConcept(page.textOwners(), selection))
      page.virtualAnnotation(annotation)
      translation["annotation_set_id"] = annotation["id"] ?? null
    }
    const pointers = [pointer, sourcePointer]
    if (selection?.["counterpart"] !== null && selection?.["counterpart"] !== undefined) {
      const counterpart = objectValue(selection["counterpart"])
      await page.original(counterpart, receiver)
      pointers.push(counterpart)
    }
    const fieldRows = pointers.flatMap((value) => {
      const row = page.rows
        .get("field_annotation")
        ?.get(
          canonicalText([value["owner"] ?? null, value["field"] ?? null, value["ordinal"] ?? null]),
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
      vocabulary: [...(page.rows.get("vocabulary")?.values() ?? [])].flatMap((row) => {
        const unit = page.rows.get("text_unit")?.get(canonicalText([row["label_unit_id"] ?? null]))
        return unit
          ? [{ kind: stringValue(row["kind"]), code: stringValue(row["code"]), unit }]
          : []
      }),
      explanations,
      cards: [...(page.rows.get("card")?.values() ?? [])]
        .filter((card) =>
          concepts.some((concept) => arrayValue(concept["card_ids"]).includes(card["id"] ?? null)),
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
  }
  // A text resolves once per snapshot: its result cannot change, and a view that mounts again
  // (switching back to the table, reopening a card) reads it without waiting a frame.
  const settled = new Map<string, SelectedText | null>()
  const running = new Map<string, Promise<SelectedText | null>>()
  return {
    peek: (pointer, targetLang) => settled.get(canonicalText([pointer, targetLang])),
    resolve: (pointer, targetLang) => {
      const id = canonicalText([pointer, targetLang])
      const value = settled.get(id)
      if (value !== undefined) return Promise.resolve(value)
      let promise = running.get(id)
      if (!promise) {
        promise = resolveText(pointer, targetLang)
          .then((result) => {
            settled.set(id, result)
            return result
          })
          .finally(() => running.delete(id))
        running.set(id, promise)
      }
      return promise
    },
  }
}
