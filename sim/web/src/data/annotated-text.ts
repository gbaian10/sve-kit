import { type CardSummary, catalogOf } from "./catalog"
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
import { type Fragment, mergedRows, type View } from "./format-v3/reader"
import { definition } from "./format-v3/schema"
import { annotationFailure, type TextOwner, TextOwners } from "./format-v3/text-owners"
import { createLocator, GLOBAL_OWNER, homeSetOwner } from "./locator"

export interface AnnotatedText {
  readonly unit: JsonObject
  readonly annotation: JsonObject | null
}

export interface SelectedText {
  readonly originalSections?: readonly { readonly ordinal: number; readonly label?: string }[]
  readonly printedState?: string
  readonly original: AnnotatedText
  readonly translated?: AnnotatedText
  readonly source?: AnnotatedText
  readonly sourceSections?: readonly AnnotatedText[]
  readonly counterpart?: AnnotatedText
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
  /** The default printing per region of each card a confirmed card-name concept points at. */
  readonly cards: readonly CardSummary[]
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

// Fragments are immutable and still carry the exact base rows used by the reader.
const bootstrapRows = new WeakMap<Fragment, ReadonlyMap<string, readonly JsonObject[]>>()
const sourceNames = new WeakMap<Fragment, ReadonlyMap<string, JsonObject>>()

function indexedRows(fragment: Fragment): ReadonlyMap<string, readonly JsonObject[]> {
  let index = bootstrapRows.get(fragment)
  if (!index) {
    const rows = new Map<string, JsonObject[]>()
    const fields = arrayValue(definition(fragment.table)["x-primary-key"]).map((field) =>
      stringValue(field),
    )
    for (const row of fragment.rows) {
      const identity = canonicalText(fields.map((field) => row[field] ?? null))
      const copies = rows.get(identity) ?? []
      copies.push(row)
      rows.set(identity, copies)
    }
    index = rows
    bootstrapRows.set(fragment, index)
  }
  return index
}

function indexedNames(fragment: Fragment): ReadonlyMap<string, JsonObject> {
  let index = sourceNames.get(fragment)
  if (!index) {
    const names = new Map<string, JsonObject>()
    for (const source of fragment.nameSources ?? []) {
      const identity = canonicalText(source["pointer"] ?? null)
      const previous = names.get(identity)
      if (previous && canonicalText(previous) !== canonicalText(source)) annotationFailure("owner")
      names.set(identity, source)
    }
    index = names
    sourceNames.set(fragment, index)
  }
  return index
}

class PageClosure {
  readonly rows = new Map<string, Map<string, JsonObject>>()
  readonly loaded = new Set<string>()
  private readonly loading = new Map<string, Promise<void>>()
  private readonly selectionFiles = new Map<string, Set<string>>()
  readonly snapshot: LoadedSnapshot
  readonly client: SnapshotClient
  readonly locate: ReturnType<typeof createLocator>
  readonly fragments: Fragment[] = []
  private readonly cardId: string | undefined
  private names: ReadonlyMap<string, JsonObject> | undefined
  private readonly printingHomes = new Map<string, JsonValue>()
  private owners: TextOwners | undefined
  private locations: ReturnType<typeof annotationLocations> | undefined

  constructor(client: SnapshotClient, snapshot: LoadedSnapshot, cardId?: string) {
    this.cardId = cardId
    this.client = client
    this.snapshot = snapshot
    this.locate = createLocator(snapshot.files)
    if (cardId === undefined) this.add(snapshot.bootstrap)
    else {
      const index = catalogOf(snapshot).index
      const revisions = new Set<JsonValue>()
      for (const face of index.facesOf(cardId)) {
        for (const raw of arrayValue(face["current"]))
          revisions.add(objectValue(raw)["revision_id"] ?? null)
        for (const raw of arrayValue(face["wording"])) {
          const wording = objectValue(raw)
          revisions.add(objectValue(wording["display"])["revision_id"] ?? null)
          for (const candidate of arrayValue(wording["candidates"]))
            revisions.add(objectValue(candidate)["revision_id"] ?? null)
        }
      }
      const homes = new Set([
        index.card(cardId)?.["home_set_id"],
        ...index.printingsOf(cardId).map((row) => row["home_set_id"]),
      ])
      const roots = snapshot.bootstrap.filter(
        (fragment) =>
          fragment.table === "vocabulary" ||
          (["card", "face", "printing", "face_revision"].includes(fragment.table) &&
            homes.has(objectValue(fragment.value["owner"])["id"])),
      )
      // Keep the physical fragments intact: exact bases refer to their original row positions.
      this.fragments.push(...snapshot.bootstrap)
      this.add(
        roots,
        (table, row) =>
          table === "card"
            ? row["id"] === cardId
            : table === "face" || table === "printing"
              ? row["card_id"] === cardId
              : table === "face_revision"
                ? revisions.has(row["id"] ?? null)
                : table === "vocabulary",
        false,
      )
      for (const row of this.rows.get("vocabulary")?.values() ?? [])
        this.includeBootstrap("text_unit", [row["label_unit_id"] ?? null])
    }
    for (const fragment of snapshot.bootstrap) this.loaded.add(fragment.file)
  }

  private includeBootstrap(table: string, primaryKey: readonly JsonValue[]): void {
    if (this.cardId === undefined) return
    if (table === "text_unit" || table === "translation") {
      const id = primaryKey[0]
      // A null reference keeps the public-annotation failure that row() reports.
      if (typeof id !== "string") return
      const index = catalogOf(this.snapshot).index
      const row = table === "text_unit" ? index.textUnit(id) : index.translation(id)
      if (row) {
        const rows = this.rows.get(table) ?? new Map<string, JsonObject>()
        const identity = canonicalText([...primaryKey])
        const previous = rows.get(identity)
        if (previous && canonicalText(previous) !== canonicalText(row))
          annotationFailure("reference")
        rows.set(identity, row)
        this.rows.set(table, rows)
      }
      return
    }
    const identity = canonicalText([...primaryKey])
    if (this.rows.get(table)?.has(identity)) return
    for (const fragment of this.snapshot.bootstrap) {
      if (fragment.table !== table) continue
      const selected = indexedRows(fragment).get(identity)
      if (selected?.length) this.add([{ ...fragment, rows: [...selected] }], undefined, false)
    }
  }

  add(
    fragments: readonly Fragment[],
    select?: (table: string, row: JsonObject) => boolean,
    retain = true,
  ): void {
    for (const fragment of fragments) {
      if (fragment.nameSources?.length) {
        this.names = undefined
        this.owners = undefined
      }
      if (fragment.table === "card_locator" || fragment.table === "__name_sources") {
        if (retain) this.fragments.push(fragment)
        continue
      }
      if (retain) this.fragments.push(fragment)
      const rows = this.rows.get(fragment.table) ?? new Map<string, JsonObject>()
      let replaced = false
      let changed = false
      const accepted = fragment.rows.filter((row) => {
        if (select) return select(fragment.table, row)
        if (this.cardId === undefined) return true
        if (fragment.table === "card") return row["id"] === this.cardId
        if (fragment.table === "face" || fragment.table === "printing")
          return row["card_id"] === this.cardId
        if (fragment.table === "face_revision")
          return this.rows.get("face")?.has(canonicalText([row["face_id"] ?? null])) === true
        return true
      })
      if (!accepted.length) continue
      for (let row of accepted) {
        this.indexSelections(row, fragment.file)
        const identity = key(fragment.table, row)
        if (
          this.cardId !== undefined &&
          ["annotation_set", "annotation_concept"].includes(fragment.table)
        )
          for (const base of this.snapshot.bootstrap) {
            if (base.table !== fragment.table) continue
            for (const original of indexedRows(base).get(identity) ?? [])
              if (canonicalText(original) !== canonicalText(row)) annotationFailure("reference")
          }
        const previous =
          rows.get(identity) ??
          (this.cardId === undefined
            ? undefined
            : fragment.table === "text_unit"
              ? catalogOf(this.snapshot).index.textUnit(stringValue(row["id"]))
              : fragment.table === "translation"
                ? catalogOf(this.snapshot).index.translation(stringValue(row["id"]))
                : undefined)
        if (
          previous &&
          ["card", "face"].includes(fragment.table) &&
          canonicalText(previous) !== canonicalText(row)
        )
          row =
            mergedRows(
              [
                { ...fragment, rows: [previous] },
                { ...fragment, rows: [row] },
              ],
              fragment.table,
            )[0] ?? row
        if (
          previous &&
          ["text_unit", "translation", "annotation_set", "annotation_concept"].includes(
            fragment.table,
          ) &&
          canonicalText(previous) !== canonicalText(row)
        )
          annotationFailure("reference")
        if (fragment.table === "printing" && fragment.value["base"] === null)
          this.printingHomes.set(
            stringValue(row["id"]),
            objectValue(fragment.value["owner"])["id"] ?? null,
          )
        if (rows.has(identity) && previous && canonicalText(previous) === canonicalText(row))
          continue
        changed = true
        replaced ||= rows.has(identity)
        rows.set(identity, row)
      }
      this.rows.set(fragment.table, rows)
      if (!changed) continue
      const placed = ["card", "face", "printing", "face_revision"].includes(fragment.table)
      if (placed || OWNER_TABLES.includes(fragment.table)) {
        if (replaced) this.owners = undefined
        else
          try {
            // Earlier owners were checked against every witness when they were indexed.
            if (this.owners)
              this.includeOwnerNames(
                this.owners,
                this.owners.include({ [fragment.table]: accepted }),
              )
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
    region: string | undefined = this.snapshot.region,
    purpose?: string,
  ): Promise<void> {
    const basic =
      table === "printing" && partition === "detail"
        ? this.fragments.find(
            (fragment) =>
              fragment.table === table &&
              fragment.value["partition"] === "bootstrap" &&
              fragment.rows.some((row) => row["id"] === primaryKey[0]) &&
              (region === undefined || fragment.value["region"] === region),
          )
        : undefined
    const segment = basic
      ? `${stringValue(basic.value["purpose"])}:${stringValue(basic.value["segment"])}`
      : undefined
    const file = this.locate({
      ...(segment ? { segment } : {}),
      table,
      owner: physicalOwner,
      entityKey,
      region,
      ...(purpose || basic ? { purpose: purpose ?? "print-detail" } : {}),
      partition,
    })
    if (file !== undefined) await this.loadFile(file)
  }

  async loadFile(file: string): Promise<void> {
    if (this.loaded.has(file)) return
    let running = this.loading.get(file)
    if (!running) {
      running = (async () => {
        const descriptor = this.snapshot.files.get(file)
        if (!descriptor) annotationFailure("reference")
        for (const raw of arrayValue(descriptor["dependencies"] ?? [])) {
          const ref = objectValue(raw)
          const target = stringValue(ref["key"])
          if (this.snapshot.files.get(target)?.["sha256"] !== ref["sha256"])
            annotationFailure("reference")
          if (this.snapshot.files.get(target)?.["role"] !== "config") await this.loadFile(target)
        }
        const fragments = await this.client.fragments(file)
        if (this.client.snapshot()?.manifestHash !== this.snapshot.manifestHash)
          throw new Error("snapshot replaced")
        this.add(fragments)
        this.loaded.add(file)
      })().finally(() => this.loading.delete(file))
      this.loading.set(file, running)
    }
    await running
  }

  private indexSelections(value: JsonValue, file: string): void {
    if (Array.isArray(value)) {
      for (const item of value) this.indexSelections(item, file)
    } else if (value !== null && typeof value === "object") {
      if ("translation_id" in value && "source" in value) {
        // Names use the catalog witness rather than source_refs.
        if (value["field"] === "name") return
        const identity = canonicalText(value)
        const files = this.selectionFiles.get(identity) ?? new Set<string>()
        files.add(file)
        this.selectionFiles.set(identity, files)
      }
      // Selections belong to rows or printing faces in the fixed schema.
      for (const field of ["translations", "faces"]) {
        const nested = value[field]
        if (nested !== undefined) this.indexSelections(nested, file)
      }
    }
  }

  async sources(selection: JsonObject): Promise<void> {
    const origins = this.selectionFiles.get(canonicalText(selection))
    if (!origins?.size) annotationFailure("owner")
    const targets = new Set<string>()
    for (const origin of origins)
      for (const raw of arrayValue(this.snapshot.files.get(origin)?.["source_refs"])) {
        const ref = objectValue(raw)
        const key = stringValue(ref["key"])
        if (this.snapshot.files.get(key)?.["sha256"] !== ref["sha256"])
          annotationFailure("reference")
        targets.add(key)
      }
    await Promise.all([...targets].map((key) => this.loadFile(key)))
  }

  async row(table: string, id: JsonValue): Promise<JsonObject> {
    const identity = canonicalText([id])
    if (!this.rows.get(table)?.has(identity)) this.includeBootstrap(table, [id])
    if (
      !this.rows.get(table)?.has(identity) &&
      !["text_unit", "translation", "annotation_set", "annotation_concept"].includes(table)
    )
      await this.load(table, [id], "detail")
    const row = this.rows.get(table)?.get(identity)
    if (!row) annotationFailure("reference")
    return row
  }

  owner(value: JsonObject) {
    return this.textOwners().get(value)
  }

  private nameSource(pointer: JsonObject): JsonObject | undefined {
    if (!this.names) {
      const names = new Map<string, JsonObject>()
      for (const fragment of this.fragments) {
        if (!fragment.nameSources?.length) continue
        for (const [identity, source] of indexedNames(fragment)) {
          const previous = names.get(identity)
          if (previous && canonicalText(previous) !== canonicalText(source))
            annotationFailure("owner")
          names.set(identity, source)
        }
      }
      this.names = names
    }
    return this.names.get(canonicalText(pointer))
  }

  private includeOwnerNames(
    owners: TextOwners,
    values: Iterable<TextOwner> = owners.byOwner.values(),
  ): void {
    if (this.names?.size === 0) return
    for (const { owner } of values) {
      if (!["face_revision", "printing_face"].includes(stringValue(owner["kind"]))) continue
      const source = this.nameSource({ owner, field: "name", ordinal: null })
      if (source) owners.includeNameSources([source])
    }
  }

  textOwners(pointer?: JsonObject): TextOwners {
    if (!this.owners) {
      const view = Object.fromEntries(
        ["card", "face", "printing", ...OWNER_TABLES].map((table) => [
          table,
          [...(this.rows.get(table)?.values() ?? [])],
        ]),
      )
      this.owners = new TextOwners(view)
      this.includeOwnerNames(this.owners)
    }
    if (pointer) {
      const source = this.nameSource(pointer)
      if (source) this.owners.includeNameSources([source])
    }
    return this.owners
  }

  location(value: JsonObject, field: string, ordinal: JsonValue = null) {
    this.owner(value)
    this.locations ??= annotationLocations(this.view(), this.printingHomes)
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
    region: string | undefined = this.snapshot.region,
  ): Promise<{ unitId: string; annotationId: JsonValue } | null> {
    const witness = this.nameSource(pointer)
    if (pointer["field"] === "name" && typeof witness?.["name_concept_id"] === "string") {
      this.textOwners(pointer)
      const unit = await this.row("text_unit", witness["text_unit_id"] ?? null)
      const annotation = wholeName(unit, witness["name_concept_id"])
      this.virtualAnnotation(annotation)
      const fields = this.rows.get("field_annotation") ?? new Map<string, JsonObject>()
      fields.set(canonicalText([pointer["owner"] ?? null, "name", null]), {
        ...pointer,
        annotation_set_id: annotation["id"] ?? null,
      })
      this.rows.set("field_annotation", fields)
      return {
        unitId: stringValue(witness["text_unit_id"]),
        annotationId: annotation["id"] ?? null,
      }
    }
    const value = objectValue(pointer["owner"])
    const field = stringValue(pointer["field"])
    const kind = stringValue(value["kind"])
    if (!OWNER_TABLES.includes(kind) && kind !== "printing_face") annotationFailure("owner")
    if (kind === "printing_face" && !this.textOwners().byOwner.has(canonicalText(value))) {
      const card = this.rows.get("card")?.get(canonicalText([receiver?.cardId ?? null]))
      if (!card) annotationFailure("owner")
      const owner = homeSetOwner(stringValue(card["home_set_id"]))
      for (const purpose of ["catalog", "print"]) {
        await this.load(
          "printing",
          [value["id"] ?? null],
          "bootstrap",
          owner,
          [value["id"] ?? null],
          region,
          purpose,
        )
        if (this.textOwners().byOwner.has(canonicalText(value))) break
      }
      await this.load(
        "printing",
        [value["id"] ?? null],
        "detail",
        owner,
        [value["id"] ?? null],
        region,
        "print-detail",
      )
    }
    if (!["face_revision", "printing_face"].includes(kind)) {
      const pk =
        kind === "vocabulary"
          ? [value["vocabulary_kind"] ?? null, value["code"] ?? null]
          : [value["id"] ?? null]
      this.includeBootstrap(kind, pk)
      await this.load(kind, pk, "detail")
    }
    if (
      kind === "face_revision" &&
      !this.rows.get(kind)?.has(canonicalText([value["id"] ?? null]))
    ) {
      const candidateFace = receiver
        ? undefined
        : [...(this.rows.get("face")?.values() ?? [])].find((face) =>
            arrayValue(face["wording"]).some((raw) =>
              arrayValue(objectValue(raw)["candidates"]).some(
                (candidate) => objectValue(candidate)["revision_id"] === value["id"],
              ),
            ),
          )
      const cardId = receiver?.cardId ?? candidateFace?.["card_id"]
      const card = this.rows.get("card")?.get(canonicalText([cardId ?? null]))
      if (!card) annotationFailure("owner")
      const physical = homeSetOwner(stringValue(card["home_set_id"]))
      const entity = [card["id"] ?? null]
      // Source owners and listed candidates live in their card's fixed bootstrap or history fragment.
      for (const partition of ["bootstrap", "history"] as const) {
        await this.load(kind, [value["id"] ?? null], partition, physical, entity, region)
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
        region,
      )
    const pk = [value, pointer["field"] ?? null, pointer["ordinal"] ?? null]
    await this.load(
      "field_annotation",
      pk,
      location.partition,
      location.physical,
      ["face_revision", "printing_face"].includes(kind) ? location.entity : pk,
      region,
    )
    const unitId = this.textOwners(pointer).optionalPointer(pointer)
    if (unitId === null) return null
    await this.row("text_unit", unitId)
    this.includeBootstrap("field_annotation", pk)
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

export function peekAnnotatedTextResolver(
  client: SnapshotClient,
): AnnotatedTextResolver | undefined {
  const snapshot = client.snapshot()
  return snapshot ? shared.get(snapshot) : undefined
}

// Share bootstrap indexing across previews and reopened card pages for the same snapshot.
// The first caller's cardId scopes it, so pass one only for a snapshot pinned to that card.
export function sharedAnnotatedTextResolver(
  client: SnapshotClient,
  cardId?: string,
): AnnotatedTextResolver {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  let resolver = shared.get(snapshot)
  if (!resolver) {
    resolver = createAnnotatedTextResolver(client, cardId)
    shared.set(snapshot, resolver)
  }
  return resolver
}

export function createAnnotatedTextResolver(
  client: SnapshotClient,
  cardId?: string,
): AnnotatedTextResolver {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const page = new PageClosure(client, snapshot, cardId)
  const resolveText = async (
    pointer: JsonObject,
    targetLang: string,
  ): Promise<SelectedText | null> => {
    if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
      throw new Error("snapshot replaced")
    const original = await page.original(pointer)
    if (original === null) return null
    const receiver = page.owner(objectValue(pointer["owner"]))
    const sections =
      pointer["field"] === "effect"
        ? await Promise.all(
            arrayValue(receiver.row["sections"] ?? []).map(async (raw) => {
              const section = objectValue(raw)
              const labelId = section["label_unit_id"]
              return {
                ordinal: integerValue(section["ordinal"]),
                ...(labelId == null
                  ? {}
                  : { label: stringValue((await page.row("text_unit", labelId))["text"]) }),
              }
            }),
          )
        : []
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
    if (
      selection?.["basis"] === "official_counterpart" &&
      (translation?.["origin"] !== "official" || translation["authority"] !== "sve_official")
    )
      annotationFailure("reference")
    const sourcePointer = selection ? objectValue(selection["source"]) : pointer
    const receiverRegion = receiver.region ?? snapshot.region
    const sourceRegion = selection?.["basis"] === "jp_source" ? "jp" : receiverRegion
    if (selection && pointer["field"] !== "name") {
      await page.sources(selection)
      page.textOwners(sourcePointer).optionalPointer(sourcePointer)
    }
    const source = await page.original(sourcePointer, receiver, sourceRegion)
    if (source === null) annotationFailure("owner")
    if (translation && translation["source_unit_id"] !== source.unitId)
      annotationFailure("reference")
    if (selection) {
      const basis = selection["basis"]
      if (basis !== "jp_source" && canonicalText(sourcePointer) !== canonicalText(pointer))
        annotationFailure("owner")
      const donor = page.textOwners().byOwner.get(canonicalText(sourcePointer["owner"] ?? null))
      if (basis === "jp_source" && donor) {
        if (
          receiver.region !== "en" ||
          donor.region !== "jp" ||
          receiver.cardId === undefined ||
          receiver.cardId !== donor.cardId ||
          receiver.owner["kind"] !== donor.owner["kind"] ||
          receiver.row["face_id"] !== donor.row["face_id"] ||
          sourcePointer["field"] !== pointer["field"]
        )
          annotationFailure("owner")
      }
    }
    if (translation?.["annotation_kind"] === "whole_name" && selection) {
      const unit = await page.row("text_unit", translation["text_unit_id"] ?? null)
      // A concurrent load may rebuild the owners after the await; re-include the source witness.
      const owners = page.textOwners(sourcePointer)
      const annotation = wholeName(unit, translationNameConcept(owners, selection))
      page.virtualAnnotation(annotation)
      translation["annotation_set_id"] = annotation["id"] ?? null
    }
    const pointers = [pointer, sourcePointer]
    let counterpart: { unitId: string; annotationId: JsonValue } | null = null
    if (selection?.["counterpart"] !== null && selection?.["counterpart"] !== undefined) {
      const pointer = objectValue(selection["counterpart"])
      if (pointer["field"] !== "name") page.textOwners(pointer).optionalPointer(pointer)
      counterpart = await page.original(pointer, receiver, receiverRegion === "jp" ? "en" : "jp")
      if (counterpart === null) annotationFailure("owner")
      const donor = page.owner(objectValue(pointer["owner"]))
      if (
        receiver.cardId === undefined ||
        receiver.cardId !== donor.cardId ||
        receiver.owner["kind"] !== donor.owner["kind"] ||
        receiver.row["face_id"] !== donor.row["face_id"] ||
        receiver.region === donor.region
      )
        annotationFailure("owner")
      if (
        translation?.["text_unit_id"] !== counterpart.unitId ||
        translation["annotation_set_id"] !== counterpart.annotationId
      )
        annotationFailure("reference")
      pointers.push(pointer)
    }
    const sourceSections = []
    if (selection && sourcePointer["field"] !== "name") {
      const sourceOwner = page.owner(objectValue(sourcePointer["owner"]))
      for (const raw of arrayValue(sourceOwner.row["sections"] ?? [])) {
        const pointer = {
          owner: sourcePointer["owner"] ?? null,
          field: "section",
          ordinal: objectValue(raw)["ordinal"] ?? null,
        }
        const section = await page.original(pointer, receiver, sourceRegion)
        if (section === null) annotationFailure("owner")
        sourceSections.push(section)
        pointers.push(pointer)
      }
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
      const annotation = id === null ? null : annotationRows.find((row) => row["id"] === id)
      if (
        annotation === undefined ||
        (annotation !== null && annotation["text_unit_id"] !== unitId)
      )
        annotationFailure("reference")
      return { unit, annotation }
    }
    return {
      ...(sections.length > 0 ? { originalSections: sections } : {}),
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
      ...(selection
        ? { sourceSections: sourceSections.map((s) => annotated(s.unitId, s.annotationId)) }
        : {}),
      ...(counterpart
        ? { counterpart: annotated(counterpart.unitId, counterpart.annotationId) }
        : {}),
      concepts,
      vocabulary: [...(page.rows.get("vocabulary")?.values() ?? [])].flatMap((row) => {
        const unit = page.rows.get("text_unit")?.get(canonicalText([row["label_unit_id"] ?? null]))
        return unit
          ? [{ kind: stringValue(row["kind"]), code: stringValue(row["code"]), unit }]
          : []
      }),
      explanations,
      cards: catalogOf(snapshot)
        .index.cards.filter((card) =>
          concepts.some((concept) => arrayValue(concept["card_ids"]).includes(card["id"] ?? null)),
        )
        .flatMap((card) =>
          arrayValue(card["regions"]).flatMap((value) => {
            const printingId = objectValue(value)["default_printing_id"]
            const summary =
              typeof printingId === "string" ? catalogOf(snapshot).summary(printingId) : undefined
            return summary ? [summary] : []
          }),
        ),
    }
  }
  // A text resolves once per snapshot: its result cannot change, and a view that mounts again
  // (switching back to the table, reopening a card) reads it without waiting a frame.
  const settled = new Map<string, SelectedText | null>()
  const running = new Map<string, Promise<SelectedText | null>>()
  return {
    peek: (pointer, targetLang) =>
      client.snapshot() === snapshot
        ? settled.get(canonicalText([pointer, targetLang]))
        : undefined,
    resolve: (pointer, targetLang) => {
      if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
        return Promise.reject(new Error("snapshot replaced"))
      const id = canonicalText([pointer, targetLang])
      const value = settled.get(id)
      if (value !== undefined) return Promise.resolve(value)
      let promise = running.get(id)
      if (!promise) {
        promise = resolveText(pointer, targetLang)
          .then((result) => {
            if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
              throw new Error("snapshot replaced")
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
