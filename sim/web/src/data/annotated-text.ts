import type { LoadedSnapshot, SnapshotClient } from "./client"
import { annotationLocations } from "./format-v3/annotation-placement"
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
      if (
        OWNER_TABLES.includes(fragment.table) ||
        ["card", "face", "printing"].includes(fragment.table)
      ) {
        this.owners = undefined
        this.locations = undefined
      }
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
  const page = new PageClosure(client, snapshot)
  return {
    resolve: async (pointer, targetLang) => {
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
      const view: View = {}
      const trim = (row: JsonObject): JsonObject => ({ ...row, translations: [] })
      const owners = pointers.map((value) => page.owner(objectValue(value["owner"])))
      const cardIds = new Set([
        ...owners.map((owner) => owner.cardId),
        ...concepts.flatMap((concept) => arrayValue(concept["card_ids"])),
      ])
      view["card"] = [...(page.rows.get("card")?.values() ?? [])].filter((row) =>
        cardIds.has(row["id"]),
      )
      view["face"] = [...(page.rows.get("face")?.values() ?? [])].filter((row) =>
        owners.some((owner) => owner.faceId === row["id"]),
      )
      view["printing"] = []
      for (const owner of owners) {
        const kind = stringValue(owner.owner["kind"])
        if (kind === "printing_face") {
          const printing = page.rows
            .get("printing")
            ?.get(canonicalText([owner.owner["id"] ?? null]))
          if (!printing) annotationFailure("owner")
          const existing = view["printing"].find((row) => row["id"] === printing["id"])
          if (existing) arrayValue(existing["faces"]).push(trim(owner.row))
          else view["printing"].push({ ...printing, faces: [trim(owner.row)] })
        } else {
          view[kind] ??= []
          if (!view[kind].some((row) => key(kind, row) === key(kind, owner.row)))
            view[kind].push(trim(owner.row))
        }
      }
      view["vocabulary"] = [...(page.rows.get("vocabulary")?.values() ?? [])].map(trim)
      const textIds = new Set<JsonValue>([
        original.unitId,
        source.unitId,
        translation?.["text_unit_id"] ?? null,
        ...annotationRows.map((row) => row["text_unit_id"] ?? null),
        ...explanations.map((value) => value.unit["id"] ?? null),
      ])
      view["text_unit"] = [...textIds]
        .filter((id) => id !== null)
        .map((id) => {
          const row = page.rows.get("text_unit")?.get(canonicalText([id]))
          if (!row) annotationFailure("reference")
          return row
        })
      for (const explanation of explanations) {
        const table = stringValue(explanation.reference["kind"])
        view[table] ??= []
        if (!view[table].some((row) => row["id"] === explanation.row["id"]))
          view[table].push(trim(explanation.row))
      }
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
        vocabulary: [...(page.rows.get("vocabulary")?.values() ?? [])].flatMap((row) => {
          const unit = page.rows
            .get("text_unit")
            ?.get(canonicalText([row["label_unit_id"] ?? null]))
          return unit
            ? [{ kind: stringValue(row["kind"]), code: stringValue(row["code"]), unit }]
            : []
        }),
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
