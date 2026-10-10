import { decodeRow, type Row } from "./decode"
import { fail } from "./errors"
import {
  arrayValue,
  canonical,
  canonicalText,
  compareBytes,
  compareCodePoints,
  integerValue,
  isObject,
  type JsonObject,
  type JsonValue,
  objectValue,
  parseStrict,
  stringValue,
  utf8,
} from "./json"
import { validateMediaDependencies, validateMediaIdentities } from "./media"
import { validatePlacement } from "./placement"
import { descriptor, primaryKey, requiredTypes, rowType, tables, validate } from "./schema"
import { validateConfig, validateFragments, validateView } from "./semantics"
import { digest } from "./sha256"

const FORMAT = "3.0.0"
/** This reader's own contract version (transport §1.1), independent of the web app version. */
const READER_CONTRACT_VERSION = "3.0.0"
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
}

export type Files = Map<string, JsonObject>

const IMAGE_TABLES = new Set(["image_asset", "printing_image", "image_variant"])
const TEXT_ROLES = new Set(["bootstrap", "text", "config"])

function reference(file: JsonObject): JsonObject {
  return { key: file["key"] ?? null, sha256: file["sha256"] ?? null }
}

function same(a: JsonValue | undefined, b: JsonValue | undefined): boolean {
  return canonicalText(a ?? null) === canonicalText(b ?? null)
}

/** Sorted by code point and unique; every member must be a string. */
function ordered(values: JsonValue[], detail: string): void {
  const strings = values.map((value) => stringValue(value))
  for (let i = 1; i < strings.length; i += 1) {
    if (compareCodePoints(strings[i - 1] ?? "", strings[i] ?? "") >= 0) {
      fail("metadata-unsorted", detail)
    }
  }
}

function versionTuple(version: string): number[] {
  return version.split(".").map((part) => Number(part))
}

function newerThan(version: string, reference: readonly number[]): boolean {
  const parts = versionTuple(version)
  for (let i = 0; i < reference.length; i += 1) {
    const a = parts[i] ?? 0
    const b = reference[i] ?? 0
    if (a !== b) return a > b
  }
  return false
}

function files(manifest: JsonObject): Files {
  const entries = arrayValue(manifest["files"] ?? null)
  const result: Files = new Map()
  for (const item of entries) {
    const file = objectValue(item)
    result.set(stringValue(file["key"]), file)
  }
  const keys = [...result.keys()]
  if (keys.length !== entries.length) fail("files-unsorted", "file keys must be unique")
  for (let i = 1; i < keys.length; i += 1) {
    if (compareCodePoints(keys[i - 1] ?? "", keys[i] ?? "") >= 0)
      fail("files-unsorted", "file keys must be sorted")
  }
  for (const [key, file] of result) {
    const dependencies = arrayValue(file["dependencies"] ?? null)
    const depKeys = dependencies.map((dep) => stringValue(objectValue(dep)["key"]))
    for (let i = 1; i < depKeys.length; i += 1) {
      if (compareCodePoints(depKeys[i - 1] ?? "", depKeys[i] ?? "") >= 0)
        fail("dependency-hash", `dependencies of ${key} must be sorted and unique`)
    }
    for (const dep of dependencies) {
      const target = result.get(stringValue(objectValue(dep)["key"]))
      if (!target || !same(dep, reference(target)))
        fail("dependency-hash", `dependency of ${key} does not match its file`)
    }
  }
  const pending = new Set(result.keys())
  while (pending.size > 0) {
    const ready = [...pending].filter((key) =>
      arrayValue(result.get(key)?.["dependencies"] ?? null).every(
        (dep) => !pending.has(stringValue(objectValue(dep)["key"])),
      ),
    )
    if (ready.length === 0) fail("dependency-cycle", "cyclic dependencies")
    for (const key of ready) pending.delete(key)
  }
  return result
}

/** Exact bytes: hash, length, content-addressed path and canonical form all have to hold. */
export function readPayload(file: JsonObject, data: Uint8Array): JsonValue {
  const hash = stringValue(file["sha256"])
  if (digest(data) !== hash || data.length !== integerValue(file["bytes"] ?? null)) {
    fail("blob-integrity", `blob hash or length mismatch for ${stringValue(file["key"])}`)
  }
  if (file["path"] !== `snapshots/blobs/${hash.slice(7)}.json`)
    fail("blob-integrity", "blob path does not match hash")
  const value = parseStrict(data)
  if (compareBytes(canonical(value), data) !== 0)
    fail("blob-integrity", "noncanonical payload bytes")
  return value
}

export function readContainer(file: JsonObject, value: JsonObject, version = "3.0.0"): Fragment[] {
  validate("Container", value, [], version)
  const key = stringValue(file["key"])
  const result: Fragment[] = []
  const used = new Set<string>()
  const counts: string[] = []
  for (const [table, entries] of Object.entries(objectValue(value["tables"] ?? null))) {
    for (const entry of arrayValue(entries)) {
      const fragment = objectValue(entry)
      const partition = stringValue(fragment["partition"])
      const role = IMAGE_TABLES.has(table)
        ? "images"
        : partition === "bootstrap"
          ? "bootstrap"
          : "text"
      if (
        role !== file["role"] ||
        !(integerValue(fragment["bucket"]) >= 0 && integerValue(fragment["bucket"]) < 64)
      ) {
        fail(
          "fragment-profile",
          `fragment ${table}/${partition} in ${key} does not match its file role or the bucket profile`,
        )
      }
      const name = rowType(table, partition, version)
      for (const nested of requiredTypes(name, version)) used.add(nested)
      const rows = arrayValue(fragment["rows"] ?? null).map((row, index) =>
        decodeRow(name, row, [key, table, index], version),
      )
      const owner = fragment["owner"] ?? null
      result.push({
        file: key,
        table,
        value: Object.fromEntries(Object.entries(fragment).filter(([name]) => name !== "rows")),
        rows,
        identity: canonicalText([table, owner, fragment["bucket"] ?? null, partition]),
      })
      counts.push(
        canonicalText({
          table,
          owner,
          bucket: fragment["bucket"] ?? null,
          partition,
          count: rows.length,
        }),
      )
    }
  }
  const expected: JsonObject = {}
  for (const name of [...used].sort(compareCodePoints)) expected[name] = descriptor(name, version)
  if (!same(value["types"], expected))
    fail("descriptor-mismatch", `missing, unused or altered nested descriptor in ${key}`)
  const declared = arrayValue(file["row_counts"] ?? null).map((item) => canonicalText(item))
  if (counts.sort().join("\n") !== declared.sort().join("\n"))
    fail("row-counts-mismatch", `fragment row counts of ${key} mismatch`)
  return result
}

function load(all: Files, payloads: ReadonlyMap<string, Uint8Array>, version: string): Fragment[] {
  const expected = [...all.keys()].sort()
  const given = [...payloads.keys()].sort()
  if (expected.join("\n") !== given.join("\n"))
    fail("payload-set", "payload set does not match manifest")
  const result: Fragment[] = []
  for (const [key, file] of all) {
    const value = objectValue(readPayload(file, payloads.get(key) ?? new Uint8Array()))
    const role = stringValue(file["role"])
    if (role === "config" || role === "programs") {
      validate(role === "config" ? "Config" : "Programs", value, [key], version)
      if (role === "config") validateConfig(value, version)
      if (arrayValue(file["row_counts"] ?? null).length !== 0)
        fail("non-table-row-counts", `${key} has row counts`)
    } else {
      result.push(...readContainer(file, value, version))
    }
  }
  if (new Set(result.map((fragment) => fragment.identity)).size !== result.length) {
    fail("duplicate-fragment", "duplicate fragment identity")
  }
  return result
}

export function findBase(detail: Fragment, fragments: Fragment[], all: Files): Fragment {
  const baseRef = objectValue(detail.value["base"] ?? null)
  const fileRef = objectValue(baseRef["file"] ?? null)
  const key = stringValue(fileRef["key"])
  const target = all.get(key)
  const dependencies = arrayValue(all.get(detail.file)?.["dependencies"] ?? null)
  if (
    key === detail.file ||
    !target ||
    !same(fileRef, reference(target)) ||
    !dependencies.some((dep) => same(dep, fileRef))
  ) {
    fail(
      "base-dependency",
      `missing or incorrect base dependency of ${detail.file}/${detail.table}`,
    )
  }
  if (
    baseRef["table"] !== detail.table ||
    !same(baseRef["owner"], detail.value["owner"]) ||
    !same(baseRef["bucket"], detail.value["bucket"])
  ) {
    fail("base-identity", `base fragment identity mismatch for ${detail.file}/${detail.table}`)
  }
  const candidates = fragments.filter(
    (f) =>
      f.file === key &&
      f.table === detail.table &&
      f.value["partition"] === "bootstrap" &&
      same(f.value["owner"], baseRef["owner"]) &&
      same(f.value["bucket"], baseRef["bucket"]),
  )
  const found = candidates[0]
  if (candidates.length !== 1 || !found)
    fail("base-missing", `base fragment not found for ${detail.file}/${detail.table}`)
  return found
}

type TranslationKey = readonly [string, number, string]

function translationKey(item: JsonValue): TranslationKey {
  const value = objectValue(item)
  const ordinal = value["ordinal"]
  return [
    stringValue(value["field"]),
    ordinal === null ? -1 : integerValue(ordinal ?? null),
    stringValue(value["target_lang"]),
  ]
}

function compareTranslation(a: TranslationKey, b: TranslationKey): number {
  return compareCodePoints(a[0], b[0]) || a[1] - b[1] || compareCodePoints(a[2], b[2])
}

function mergeTranslations(left: JsonValue | undefined, right: JsonValue | undefined): JsonValue[] {
  const merged = [...arrayValue(left ?? null), ...arrayValue(right ?? null)]
  const keyed = merged.map((item) => ({ key: translationKey(item), item }))
  if (new Set(keyed.map(({ key }) => key.join("\u0000"))).size !== keyed.length) {
    fail("translation-duplicate", "duplicate translation selection")
  }
  return keyed.sort((a, b) => compareTranslation(a.key, b.key)).map(({ item }) => item)
}

function joinPrinting(baseRow: Row, detailRow: Row, faces: Map<string, Row>): Row {
  const left = arrayValue(baseRow["faces"] ?? null).map((item) => objectValue(item))
  const right = arrayValue(detailRow["faces"] ?? null).map((item) => objectValue(item))
  const ordinals = left.map((item) =>
    integerValue(faces.get(stringValue(item["face_id"]))?.["ordinal"] ?? null),
  )
  const sortedUnique = [...new Set(ordinals)].sort((a, b) => a - b)
  if (
    ordinals.join(",") !== sortedUnique.join(",") ||
    right.map((item) => integerValue(item["face_ordinal"] ?? null)).join(",") !== ordinals.join(",")
  ) {
    fail("face-ordinal", "printing faces do not match exact permanent ordinals")
  }
  const merged: JsonValue[] = left.map((first, index) => {
    const second = right[index] ?? {}
    if (!same(faces.get(stringValue(first["face_id"]))?.["card_id"], baseRow["card_id"])) {
      fail("face-card-mismatch", "printing face belongs to another card")
    }
    const { face_ordinal: _ordinal, ...rest } = second
    return { ...first, ...rest }
  })
  return { ...baseRow, faces: merged }
}

export function joinDetail(
  detail: Fragment,
  baseFragment: Fragment,
  faces: Map<string, Row>,
): Row[] {
  const indexes = detail.rows.map((row) => integerValue(row["row_index"] ?? null))
  if (
    indexes.length !== baseFragment.rows.length ||
    indexes.some((value, index) => value !== index)
  ) {
    fail(
      "row-index",
      `missing, duplicate, unordered or out-of-range row_index in ${detail.file}/${detail.table}`,
    )
  }
  return baseFragment.rows.map((first, index) => {
    const second = detail.rows[index] ?? {}
    if (detail.table === "printing") return joinPrinting(first, second, faces)
    const leftTranslations = arrayValue(first["translations"] ?? null)
    const rightTranslations = arrayValue(second["translations"] ?? null)
    if (
      leftTranslations.some((item) => objectValue(item)["field"] !== "name") ||
      rightTranslations.some((item) => objectValue(item)["field"] === "name")
    ) {
      fail("translation-partition", "translations placed in the wrong column partition")
    }
    const { row_index: _index, ...rest } = second
    return {
      ...first,
      ...rest,
      translations: mergeTranslations(first["translations"], second["translations"]),
    }
  })
}

function logical(fragments: Fragment[], all: Files): View {
  const view: View = {}
  for (const table of tables()) view[table] = []
  const faces = new Map<string, Row>()
  for (const fragment of fragments) {
    if (fragment.table === "face")
      for (const row of fragment.rows) faces.set(stringValue(row["id"]), row)
  }
  const joined = new Set<string>()
  for (const fragment of fragments) {
    const partition = fragment.value["partition"]
    if (
      (fragment.table === "printing" || fragment.table === "face_revision") &&
      partition !== "history"
    ) {
      if (partition === "bootstrap") continue
      const baseFragment = findBase(fragment, fragments, all)
      if (joined.has(baseFragment.identity))
        fail("multiple-details", `multiple details for one base in ${fragment.table}`)
      joined.add(baseFragment.identity)
      view[fragment.table]?.push(...joinDetail(fragment, baseFragment, faces))
    } else {
      view[fragment.table]?.push(...fragment.rows)
    }
  }
  const expected = fragments
    .filter(
      (f) =>
        (f.table === "printing" || f.table === "face_revision") &&
        f.value["partition"] === "bootstrap",
    )
    .map((f) => f.identity)
  if (expected.some((identity) => !joined.has(identity)))
    fail("missing-detail", "missing detail fragment")
  return view
}

function keyBytes(row: Row, fields: readonly string[]): Uint8Array {
  return canonical(fields.map((field) => row[field] ?? null))
}

function unique(view: View): void {
  for (const [table, rows] of Object.entries(view)) {
    const fields = primaryKey(table)
    const keyed = rows.map((row) => ({ key: keyBytes(row, fields), row }))
    if (new Set(keyed.map(({ key }) => key.join(","))).size !== keyed.length) {
      fail("primary-key-duplicate", `duplicate public primary key in ${table}`)
    }
    keyed.sort((a, b) => compareBytes(a.key, b.key))
    view[table] = keyed.map(({ row }) => row)
  }
}

const REFERENCES: Readonly<Record<string, string>> = {
  card_id: "card",
  from_card_id: "card",
  to_card_id: "card",
  old_card_id: "card",
  new_card_id: "card",
  face_id: "face",
  printing_id: "printing",
  default_printing_id: "printing",
  art_id: "art",
  artist_id: "artist",
  stamp_id: "stamp",
  home_set_id: "product_family",
  family_id: "product_family",
  product_id: "product",
  translation_id: "translation",
  annotation_set_id: "annotation_set",
  qa_id: "qa",
  qa_version_id: "qa_version",
  current_version_id: "qa_version",
  cr_version_id: "cr_version",
  cr_clause_id: "cr_clause",
  revision_id: "face_revision",
  replacement_revision_id: "ruling_revision",
  rules_name_id: "rules_name",
  profile_id: "rules_profile",
  digital_card_id: "digital_card",
  digital_art_id: "digital_art",
  interaction_target_id: "digital_card",
  voice_id: "voice",
  keyword_id: "keyword",
  image_id: "image_asset",
}
const ARRAY_REFERENCES: Readonly<Record<string, string>> = {
  card_ids: "card",
  cards: "card",
  debut_product_ids: "product",
  active_scopes: "text_unit",
  ruling_revision_ids: "ruling_revision",
  complete_keyword_ids: "keyword",
  partial_keyword_ids: "keyword",
  undated_printing_ids: "printing",
}

function walk(value: JsonValue, targets: Map<string, Set<string>>): void {
  if (Array.isArray(value)) {
    for (const item of value) walk(item, targets)
    return
  }
  if (!isObject(value)) return
  for (const [key, item] of Object.entries(value)) {
    const target = key.endsWith("_unit_id") ? "text_unit" : REFERENCES[key]
    if (target !== undefined && item !== null && !targets.get(target)?.has(stringValue(item))) {
      fail("dangling-reference", `dangling reference ${key}`)
    }
    const arrayTarget = ARRAY_REFERENCES[key]
    if (
      arrayTarget !== undefined &&
      arrayValue(item).some((member) => !targets.get(arrayTarget)?.has(stringValue(member)))
    ) {
      fail("dangling-reference", `dangling array reference ${key}`)
    }
    if (key === "program_ref" && item !== null)
      fail("program-ref-unsupported", "no programs supported in this profile")
    walk(item, targets)
  }
}

function closure(view: View, manifest: JsonObject): void {
  const targets = new Map<string, Set<string>>()
  for (const [table, rows] of Object.entries(view)) {
    targets.set(
      table,
      new Set(rows.filter((row) => "id" in row).map((row) => stringValue(row["id"]))),
    )
  }
  for (const rows of Object.values(view)) for (const row of rows) walk(row, targets)
  const faces = [...(view["face"] ?? [])].sort(
    (a, b) => integerValue(a["ordinal"] ?? null) - integerValue(b["ordinal"] ?? null),
  )
  for (const card of view["card"] ?? []) {
    const actual = faces
      .filter((row) => same(row["card_id"], card["id"]))
      .map((row) => row["id"] ?? null)
    if (!same(card["faces"], actual)) fail("card-faces-mismatch", "card face closure mismatch")
  }
  for (const key of ["qa_card_ids", "errata_card_ids"]) {
    const ids = arrayValue(manifest[key] ?? null)
    ordered(ids, `${key} must be sorted and unique`)
    if (ids.some((item) => !targets.get("card")?.has(stringValue(item))))
      fail("manifest-card-reference", `${key} references a missing card`)
  }
  for (const row of view["text_unit"] ?? []) {
    const expected = `t:${stringValue(row["lang"])}:${digest(utf8(stringValue(row["text"]))).slice(7, 23)}`
    if (row["id"] !== expected) fail("text-id-mismatch", "text ID does not match exact text")
  }
}

function current(view: View, fragments: Fragment[]): void {
  const refs = new Set<string>()
  for (const row of view["face"] ?? []) {
    for (const item of arrayValue(row["current"] ?? null))
      refs.add(stringValue(objectValue(item)["revision_id"]))
    for (const item of arrayValue(row["wording"] ?? [])) {
      const display = objectValue(objectValue(item)["display"])
      if (display["revision_id"] !== null) refs.add(stringValue(display["revision_id"]))
    }
  }
  const currentIds = new Set<string>()
  for (const fragment of fragments) {
    if (fragment.table === "face_revision" && fragment.value["partition"] === "bootstrap") {
      for (const row of fragment.rows) currentIds.add(stringValue(row["id"]))
    }
  }
  if (refs.size !== currentIds.size || [...refs].some((id) => !currentIds.has(id))) {
    fail("current-history-mismatch", "current/history partition mismatch")
  }
  const revisions = new Map(
    (view["face_revision"] ?? []).map((row) => [stringValue(row["id"]), row]),
  )
  for (const face of view["face"] ?? []) {
    for (const item of arrayValue(face["current"] ?? null)) {
      const entry = objectValue(item)
      const revision = revisions.get(stringValue(entry["revision_id"]))
      if (
        !revision ||
        !same(revision["face_id"], face["id"]) ||
        !same(revision["region"], entry["region"])
      ) {
        fail("current-face-mismatch", "current revision belongs to another face or region")
      }
    }
  }
}

function metadata(manifest: JsonObject, all: Files): void {
  for (const field of ["regions", "languages", "required_capabilities"]) {
    ordered(arrayValue(manifest[field] ?? null), `${field} must be sorted and unique`)
  }
  const textFiles = new Set(
    [...all].filter(([, file]) => TEXT_ROLES.has(stringValue(file["role"]))).map(([key]) => key),
  )
  const textAll = manifest["text_all"]
  if (textAll !== null && textAll !== undefined) {
    const expected = [...textFiles]
      .sort(compareCodePoints)
      .map((key) => reference(all.get(key) ?? {}))
    if (!same(objectValue(textAll)["contains"], expected))
      fail("text-all-membership", "text_all contains does not match files")
  }
  for (const [key, file] of all) {
    const dependencies = arrayValue(file["dependencies"] ?? null).map((dep) =>
      stringValue(objectValue(dep)["key"]),
    )
    if (textFiles.has(key) && dependencies.some((dep) => !textFiles.has(dep)))
      fail("dependency-closure", `text dependencies of ${key} escape the offline closure`)
    if (
      file["role"] === "bootstrap" &&
      dependencies.some((dep) => all.get(dep)?.["role"] === "text")
    )
      fail("bootstrap-dependency", "bootstrap cannot depend on detail/history")
    if (file["role"] === "programs" && dependencies.length > 0)
      fail("programs-dependency", "empty programs cannot have dependencies")
  }
}

/** Whether a manifest or version-index entry can be read by this reader (transport §1.1). */
export function isCompatible(
  entry: JsonObject,
  reader: {
    readonly version: string
    readonly formats: readonly string[]
    readonly capabilities: readonly string[]
  } = { version: READER_CONTRACT_VERSION, formats: [FORMAT], capabilities: CAPABILITIES },
): boolean {
  const capabilities = arrayValue(entry["required_capabilities"] ?? null).map((item) =>
    stringValue(item),
  )
  return (
    reader.formats.includes(stringValue(entry["format_version"])) &&
    !newerThan(stringValue(entry["min_reader_version"]), versionTuple(reader.version)) &&
    capabilities.length === CAPABILITIES.length &&
    CAPABILITIES.every((capability) => capabilities.includes(capability)) &&
    capabilities.every((capability) => reader.capabilities.includes(capability))
  )
}

/** Everything the manifest alone can prove: shape, compatibility, files, dependencies, metadata. */
export function verifyManifest(manifestValue: JsonValue): { manifest: JsonObject; files: Files } {
  const manifest = objectValue(manifestValue)
  const version = stringValue(manifest["format_version"])
  if (version !== FORMAT) fail("unsupported-version", "unsupported format")
  validate("Manifest", manifestValue, [], version)
  if (!isCompatible(manifest))
    fail("unsupported-version", "unsupported format, reader version or capability")
  const all = files(manifest)
  const identities = new Set<string>()
  for (const file of all.values()) {
    for (const count of arrayValue(file["row_counts"])) {
      const entry = objectValue(count)
      const identity = canonicalText([
        entry["table"] ?? null,
        entry["owner"] ?? null,
        entry["bucket"] ?? null,
        entry["partition"] ?? null,
      ])
      if (identities.has(identity))
        fail("duplicate-fragment", "manifest fragment identity is not unique")
      identities.add(identity)
      validatePlacement(
        [
          {
            file: stringValue(file["key"]),
            table: stringValue(entry["table"]),
            rows: [],
            identity,
            value: { ...entry, base: null },
          },
        ],
        version,
      )
    }
  }
  metadata(manifest, all)
  const configs = [...all.values()].filter((file) => file["role"] === "config")
  const programs = [...all.values()].filter((file) => file["role"] === "programs")
  const config = configs[0]
  if (
    configs.length !== 1 ||
    !config ||
    !same(manifest["config_ref"], reference(config)) ||
    programs.length !== 1
  ) {
    fail("config-programs-count", "expected exactly one config and one programs file")
  }
  for (const file of all.values()) {
    if (
      file["role"] === "images" &&
      !arrayValue(file["dependencies"] ?? null).some((dep) => same(dep, manifest["config_ref"]))
    ) {
      fail("images-config-dependency", "images must depend on config")
    }
  }
  return { manifest, files: all }
}

/**
 * Verify every file before joining a complete snapshot into logical rows. Missing keys and invalid
 * shapes propagate as `SnapshotError`; nothing is repaired and no other snapshot is consulted.
 */
export function readSnapshot(
  manifestValue: JsonValue,
  payloads: ReadonlyMap<string, Uint8Array>,
): View {
  const { manifest, files: all } = verifyManifest(manifestValue)
  const fragments = load(all, payloads, stringValue(manifest["format_version"]))
  validateFragments(fragments)
  const view = logical(fragments, all)
  unique(view)
  closure(view, manifest)
  current(view, fragments)
  validateMediaIdentities(view)
  validateMediaDependencies(fragments, all, stringValue(objectValue(manifest["config_ref"])["key"]))
  validateView(view, manifest, fragments)
  validatePlacement(fragments, stringValue(manifest["format_version"]))
  return view
}

/** Validate the alternative container's member bytes, then use the same independent reader. */
export function readTextAll(
  manifestValue: JsonValue,
  data: Uint8Array,
  attachments: ReadonlyMap<string, Uint8Array>,
): View {
  const manifest = objectValue(manifestValue)
  const version = stringValue(manifest["format_version"])
  if (version !== FORMAT) fail("unsupported-version", "unsupported format")
  validate("Manifest", manifestValue, [], version)
  const description = objectValue(manifest["text_all"] ?? null)
  const value = objectValue(readPayload(description, data))
  validate("TextAll", value, [], version)
  const all = files(manifest)
  const expected = [...all.values()]
    .filter((file) => TEXT_ROLES.has(stringValue(file["role"])))
    .map(reference)
  if (!same(description["contains"], expected))
    fail("text-all-membership", "text_all membership mismatch")
  const members = arrayValue(value["members"] ?? null).map((item) => objectValue(item))
  if (!same(members.map(reference), expected))
    fail("text-all-payload", "text_all members do not match contains")
  const inside = new Set(expected.map((item) => stringValue(item["key"])))
  for (const item of expected) {
    const file = all.get(stringValue(item["key"]))
    for (const dep of arrayValue(file?.["dependencies"] ?? null)) {
      if (!inside.has(stringValue(objectValue(dep)["key"])))
        fail("dependency-closure", "text_all dependencies escape its closure")
    }
  }
  const payloads = new Map<string, Uint8Array>()
  for (const member of members)
    payloads.set(stringValue(member["key"]), canonical(member["payload"] ?? null))
  for (const key of attachments.keys()) {
    if (payloads.has(key)) fail("text-all-payload", `duplicate alternative payload ${key}`)
  }
  return readSnapshot(manifest, new Map([...payloads, ...attachments]))
}
