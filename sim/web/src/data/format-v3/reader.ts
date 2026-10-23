import { drain } from "./cursor"
import { decodeRow, type Row } from "./decode"
import { fail } from "./errors"
import {
  arrayValue,
  canonicalText,
  compareCodePoints,
  integerValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./json"
import { primaryKey, rowType } from "./schema"

const FORMAT = "4.0.0"
/** This reader's own contract version (transport §1.1), independent of the web app version. */
const READER_CONTRACT_VERSION = "4.0.2"
const CAPABILITIES: readonly string[] = [
  "column-partition-v1",
  "fragment-container-v1",
  "image-entity-buckets-v1",
  "rules-name-on-demand-v1",
  "digital-same-name-links-v1",
  "image-id-url-v1",
  "jp-source-translation-v1",
  "public-annotation-v1",
]

export type View = Record<string, Row[]>

export interface Fragment {
  readonly file: string
  readonly table: string
  readonly value: JsonObject
  readonly rows: Row[]
  /** canonical [table, owner, bucket, partition] */
  readonly identity: string
  readonly nameSources?: readonly JsonObject[]
}

export type Files = Map<string, JsonObject>

/** Whether a manifest or version-index entry can be read by this reader (transport §1.1). */
export function isCompatible(entry: JsonObject): boolean {
  const capabilities = arrayValue(entry["required_capabilities"] ?? null).map((item) =>
    stringValue(item),
  )
  return (
    stringValue(entry["format_version"]) === FORMAT &&
    entry["min_reader_version"] === READER_CONTRACT_VERSION &&
    capabilities.length === CAPABILITIES.length &&
    CAPABILITIES.every((capability) => capabilities.includes(capability)) &&
    capabilities.every((capability) => CAPABILITIES.includes(capability))
  )
}

/** Decode an already hash-verified published manifest. */
export function readManifest(value: JsonValue): { manifest: JsonObject; files: Files } {
  const manifest = objectValue(value)
  if (!isCompatible(manifest))
    fail("unsupported-version", "unsupported format, reader version or capability")
  const files = new Map(
    arrayValue(manifest["files"]).map((item) => {
      const file = objectValue(item)
      return [stringValue(file["key"]), file] as const
    }),
  )
  const purposes = [
    "catalog",
    "print",
    "detail",
    "print-detail",
    "product",
    "history",
    "engine",
    "region-state",
    "translation",
    "images",
    "config",
    "programs",
  ]
  if (files.size !== arrayValue(manifest["files"]).length) fail("shape", "duplicate file key")
  for (const file of files.values()) {
    if (
      !purposes.includes(stringValue(file["purpose"])) ||
      ![null, "jp", "en"].includes(file["region"] as string | null)
    )
      fail("shape", "unknown File scope")
    const purpose = stringValue(file["purpose"])
    const role =
      purpose === "catalog"
        ? "bootstrap"
        : ["config", "programs", "images"].includes(purpose)
          ? purpose
          : "text"
    if (file["role"] !== role) fail("shape", "File role differs from purpose")
    if (
      ["config", "programs", "images", "translation"].includes(purpose) &&
      file["region"] !== null
    )
      fail("shape", "shared File region scope")
    stringValue(file["segment"])
  }
  for (const file of files.values()) {
    for (const field of ["dependencies", "source_refs"]) {
      const refs = arrayValue(file[field])
      const keys = refs.map((raw) => stringValue(objectValue(raw)["key"]))
      if (keys.some((key, i) => i > 0 && compareCodePoints(keys[i - 1] ?? "", key) >= 0))
        fail("shape", "file references must be sorted and unique")
      for (const raw of refs) {
        const ref = objectValue(raw)
        const target = files.get(stringValue(ref["key"]))
        if (
          !target ||
          target["sha256"] !== ref["sha256"] ||
          (field === "source_refs" && !["text", "bootstrap"].includes(stringValue(target["role"])))
        )
          fail("shape", "invalid exact file reference")
      }
    }
  }
  const visited = new Set<string>()
  const visiting = new Set<string>()
  const visit = (key: string): void => {
    if (visiting.has(key)) fail("shape", "cyclic dependencies")
    if (visited.has(key)) return
    visiting.add(key)
    for (const raw of arrayValue(files.get(key)?.["dependencies"]))
      visit(stringValue(objectValue(raw)["key"]))
    visiting.delete(key)
    visited.add(key)
  }
  for (const key of files.keys()) visit(key)
  return { manifest, files }
}

export function readContainer(file: JsonObject, value: JsonObject, version = "4.0.0"): Fragment[] {
  const key = stringValue(file["key"])
  const counts: JsonValue[] = []
  const result: Fragment[] = Object.entries(objectValue(value["tables"])).flatMap(
    ([table, entries]) =>
      arrayValue(entries).map((entry) => {
        const packed = arrayValue(entry)
        if (packed.length !== 5) fail("shape", "fragment tuple length")
        const owner = arrayValue(packed[0])
        if (
          owner.length !== 2 ||
          !["global", "home_set"].includes(stringValue(owner[0])) ||
          (owner[0] === "global") !== (owner[1] === null)
        )
          fail("shape", "fragment owner")
        const partition = stringValue(packed[1])
        const segment = stringValue(packed[2])
        const rows = arrayValue(packed[4]).map((row) =>
          decodeRow(rowType(table, partition, version), row, [], version),
        )
        const metadata: JsonObject = {
          file_sha256: file["sha256"] ?? null,
          owner: { kind: owner[0] ?? null, id: owner[1] ?? null },
          partition,
          segment,
          base: packed[3] ?? null,
          region: file["region"] ?? null,
          purpose: file["purpose"] ?? null,
        }
        counts.push([table, owner[0] ?? null, owner[1] ?? null, partition, segment, rows.length])
        if (
          table === "card_mentioned_by" &&
          (file["purpose"] !== "engine" || file["region"] !== null)
        )
          fail("shape", "card-name back references require shared engine")
        for (const row of rows) {
          if (
            table !== "card_mentioned_by" &&
            row["region"] !== undefined &&
            row["region"] !== file["region"] &&
            file["purpose"] !== "images"
          )
            fail("shape", "row region differs from File scope")
          for (const field of table === "card"
            ? ["regions"]
            : table === "face"
              ? ["current", "wording"]
              : [])
            if (arrayValue(row[field]).some((raw) => objectValue(raw)["region"] !== file["region"]))
              fail("shape", "regional row escapes File scope")
        }
        const keys = rows.map((row) =>
          canonicalText(
            (metadata["base"] === null ? primaryKey(table) : ["row_index"]).map(
              (k) => row[k] ?? null,
            ),
          ),
        )
        if (new Set(keys).size !== keys.length) fail("shape", "duplicate fragment row")
        return {
          file: key,
          table,
          value: metadata,
          rows,
          identity: canonicalText([
            key,
            table,
            metadata["owner"] ?? null,
            file["region"] ?? null,
            file["purpose"] ?? null,
            segment,
            partition,
          ]),
        }
      }),
  )
  if (new Set(result.map((f) => f.identity)).size !== result.length)
    fail("shape", "duplicate fragment identity")
  if (canonicalText(counts) !== canonicalText(file["row_counts"] ?? null))
    fail("shape", "fragment row counts")
  const nameSources = arrayValue(value["name_sources"] ?? []).map((raw) => {
    const row = arrayValue(raw)
    if (row.length !== 3) fail("shape", "name source tuple length")
    return {
      pointer: decodeRow("PublicTextPointer", row[0] ?? null),
      text_unit_id: row[1] ?? null,
      name_concept_id: row[2] ?? null,
    }
  })
  if (nameSources.length)
    result.push({
      file: key,
      table: "__name_sources",
      value: { base: null, purpose: file["purpose"] ?? null, region: file["region"] ?? null },
      rows: [],
      identity: `${key}/name-sources`,
      nameSources,
    })
  for (const [family, values] of Object.entries(objectValue(value["card_locators"] ?? {}))) {
    if (file["purpose"] !== "catalog" || file["segment"] !== `family-${family}`)
      fail("shape", "card locator scope")
    const rows = arrayValue(values).map((raw) => {
      const row = arrayValue(raw)
      if (row.length !== 3) fail("shape", "card locator tuple length")
      return {
        id: stringValue(row[0]),
        card_id: stringValue(row[1]),
        card_no: stringValue(row[2]),
        region: file["region"] ?? null,
        home_set_id: family,
      }
    })
    result.push({
      file: key,
      table: "card_locator",
      value: {
        owner: { kind: "home_set", id: family },
        partition: "locator",
        segment: file["segment"] ?? null,
        region: file["region"] ?? null,
        purpose: "catalog",
        base: null,
      },
      rows,
      identity: `${key}/card-locators`,
    })
  }
  return result
}

export function mergedRows(fragments: readonly Fragment[], table: string): Row[] {
  return drain(mergedRowsCursor(fragments, table))
}

export function* mergedRowsCursor(
  fragments: readonly Fragment[],
  table: string,
): Generator<void, Row[]> {
  const result = new Map<string, Row>()
  let visited = 0
  let fields: readonly string[] | undefined
  const regional = table === "card" ? ["regions"] : table === "face" ? ["current", "wording"] : []
  for (const fragment of fragments) {
    if (fragment.table !== table) continue
    for (const row of fragment.rows) {
      if ((++visited & 31) === 0) yield
      fields ??= primaryKey(table)
      const key = canonicalText(fields.map((field) => row[field] ?? null))
      const previous = result.get(key)
      if (!previous) {
        result.set(key, row)
        continue
      }
      if (!regional.length) {
        if (canonicalText(previous) !== canonicalText(row)) fail("shape", "conflicting row copy")
        continue
      }
      const fixed = (value: Row) =>
        Object.fromEntries(Object.entries(value).filter(([k]) => !regional.includes(k)))
      if (canonicalText(fixed(previous)) !== canonicalText(fixed(row)))
        fail("shape", "conflicting fixed regional fields")
      const merged = { ...previous }
      for (const field of regional) {
        const values = [...arrayValue(previous[field]), ...arrayValue(row[field])]
        const regions = values.map((v) => stringValue(objectValue(v)["region"]))
        if (new Set(regions).size !== regions.length) fail("shape", "duplicate regional row")
        merged[field] = values.sort((a, b) =>
          compareCodePoints(
            stringValue(objectValue(a)["region"]),
            stringValue(objectValue(b)["region"]),
          ),
        )
      }
      result.set(key, merged)
    }
  }
  yield
  return [...result.values()]
}

export function findBase(detail: Fragment, fragments: readonly Fragment[]): Fragment {
  const ref = objectValue(detail.value["base"])
  const key = stringValue(objectValue(ref["file"])["key"])
  if (
    ref["table"] !== detail.table ||
    canonicalText(ref["owner"] ?? null) !== canonicalText(detail.value["owner"] ?? null)
  )
    fail("base-missing", "base owner or table mismatch")
  const found = fragments.find(
    (fragment) =>
      fragment.file === key &&
      fragment.value["file_sha256"] === objectValue(ref["file"])["sha256"] &&
      fragment.table === detail.table &&
      fragment.value["partition"] === "bootstrap" &&
      fragment.value["segment"] === ref["segment"] &&
      fragment.value["region"] === detail.value["region"] &&
      canonicalText(fragment.value["owner"] ?? null) === canonicalText(ref["owner"] ?? null),
  )
  if (!found) fail("base-missing", "detail bootstrap is missing")
  return found
}

export function joinDetail(detail: Fragment, base: Fragment, faces: Map<string, Row>): Row[] {
  if (
    detail.rows.length !== base.rows.length ||
    detail.rows.some((row, index) => row["row_index"] !== index)
  )
    fail("row-index", "incomplete or invalid exact row indices")
  return detail.rows.map((row) => {
    const first = base.rows[integerValue(row["row_index"])]
    if (!first) fail("row-index", "detail bootstrap row is missing")
    const { row_index: _index, ...rest } = row
    if (detail.table === "printing") {
      const faceRows = arrayValue(row["faces"])
      const ordinals = faceRows.map((value) => integerValue(objectValue(value)["face_ordinal"]))
      if (
        faceRows.length !== arrayValue(first["faces"]).length ||
        new Set(ordinals).size !== ordinals.length
      )
        fail("face-ordinal", "duplicate or incomplete printing face details")
      const details = new Map(
        faceRows.map((raw) => {
          const face = objectValue(raw)
          const { face_ordinal: _ordinal, ...fields } = face
          return [integerValue(face["face_ordinal"]), fields] as const
        }),
      )
      return {
        ...first,
        faces: arrayValue(first["faces"]).map((raw) => {
          const face = objectValue(raw)
          const fields = details.get(
            integerValue(faces.get(stringValue(face["face_id"]))?.["ordinal"]),
          )
          if (!fields) fail("face-ordinal", "printing face detail is missing")
          return { ...face, ...fields }
        }),
      }
    }
    const translations = [...arrayValue(first["translations"]), ...arrayValue(rest["translations"])]
    translations.sort((a, b) => {
      const left = objectValue(a),
        right = objectValue(b)
      return (
        compareCodePoints(stringValue(left["field"]), stringValue(right["field"])) ||
        (left["ordinal"] === null ? -1 : integerValue(left["ordinal"])) -
          (right["ordinal"] === null ? -1 : integerValue(right["ordinal"])) ||
        compareCodePoints(stringValue(left["target_lang"]), stringValue(right["target_lang"]))
      )
    })
    return { ...first, ...rest, translations }
  })
}

export function catalogKeys(files: Files, region: string): string[] {
  const keys = new Set(
    [...files]
      .filter(
        ([, file]) =>
          file["purpose"] === "config" ||
          (file["purpose"] === "catalog" && (file["region"] === null || file["region"] === region)),
      )
      .map(([key]) => key),
  )
  const pending = [...keys]
  while (pending.length) {
    const key = pending.pop()
    const file = files.get(key ?? "")
    if (!file) fail("shape", "missing catalog dependency")
    for (const raw of arrayValue(file["dependencies"])) {
      const ref = objectValue(raw)
      const target = stringValue(ref["key"])
      const dependency = files.get(target)
      if (!dependency || dependency["sha256"] !== ref["sha256"])
        fail("shape", "catalog dependency hash")
      if (!["catalog", "config"].includes(stringValue(dependency["purpose"])))
        fail("shape", "catalog dependency escapes basic catalog")
      if (!keys.has(target)) {
        keys.add(target)
        pending.push(target)
      }
    }
  }
  return [...keys].sort(compareCodePoints)
}

export function validateCatalog(fragments: readonly Fragment[]): void {
  drain(validateCatalogCursor(fragments))
}

export function* validateCatalogCursor(fragments: readonly Fragment[]): Generator<void, void> {
  let visited = 0
  const texts = new Set<string>(),
    translations = new Set<string>(),
    revisions = new Set<string>()
  for (const [table, ids] of [
    ["text_unit", texts],
    ["translation", translations],
    ["face_revision", revisions],
  ] as const) {
    for (const row of yield* mergedRowsCursor(fragments, table)) {
      ids.add(stringValue(row["id"]))
      if ((++visited & 31) === 0) yield
    }
  }
  for (const face of yield* mergedRowsCursor(fragments, "face")) {
    const ids = [
      ...arrayValue(face["current"]).map((r) => objectValue(r)["revision_id"]),
      ...arrayValue(face["wording"]).map(
        (r) => objectValue(objectValue(r)["display"])["revision_id"],
      ),
    ].filter((id) => id !== null)
    if (ids.some((id) => !revisions.has(stringValue(id)))) fail("shape", "missing catalog revision")
    if ((++visited & 31) === 0) yield
  }
  type Frame =
    | { kind: "array"; items: JsonValue[]; offset: number }
    | { kind: "object"; items: [string, JsonValue][]; offset: number }
  const stack: Frame[] = []
  const push = (item: JsonValue): void => {
    if (Array.isArray(item)) stack.push({ kind: "array", items: item, offset: 0 })
    else if (item !== null && typeof item === "object")
      stack.push({ kind: "object", items: Object.entries(item), offset: 0 })
  }
  for (const fragment of fragments) {
    if (fragment.value["purpose"] !== "catalog") continue
    for (const row of fragment.rows) {
      push(row)
      while (stack.length) {
        const frame = stack[stack.length - 1]
        if (!frame) break
        if (frame.offset === frame.items.length) {
          stack.pop()
          continue
        }
        let item: JsonValue
        if (frame.kind === "array") item = frame.items[frame.offset++] ?? null
        else {
          const entry = frame.items[frame.offset++]
          if (!entry) continue
          const [field, child] = entry
          if (field.endsWith("_unit_id") && typeof child === "string" && !texts.has(child))
            fail("shape", "missing catalog text")
          if (field === "translation_id" && typeof child === "string" && !translations.has(child))
            fail("shape", "missing catalog translation")
          item = child
        }
        push(item)
        // Small rows share checkpoints without making deeply nested references atomic.
        if ((++visited & 31) === 0) yield
      }
    }
    for (const source of fragment.nameSources ?? []) {
      if (!texts.has(stringValue(source["text_unit_id"])))
        fail("shape", "missing catalog source text")
      if ((++visited & 31) === 0) yield
    }
  }
  yield
}
