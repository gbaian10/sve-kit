import { fail } from "./errors"
import {
  arrayValue,
  canonicalText,
  compareCodePoints,
  integerValue,
  isObject,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
  utf8,
} from "./json"
import type { Fragment, View } from "./reader"
import { primaryKey } from "./schema"
import { digest } from "./sha256"

type Row = JsonObject

const CARD_TABLES = new Set([
  "card",
  "face",
  "face_revision",
  "card_engine_support",
  "mechanic_projection",
  "card_mechanic_coverage",
  "card_related",
  "digital_link",
  "digital_link_coverage",
  "card_voice",
])
const PRINT_TABLES = new Set(["printing", "printing_product", "printing_image"])
const ART_TABLES = new Set(["art", "digital_art_link"])

// null < string < integer, then by value; the same total order the Python reference uses.
function compareKeyValue(a: JsonValue | undefined, b: JsonValue | undefined): number {
  const rank = (value: JsonValue | undefined): number =>
    value === null || value === undefined ? 0 : typeof value === "string" ? 1 : 2
  const ra = rank(a)
  const rb = rank(b)
  if (ra !== rb) return ra - rb
  if (typeof a === "string" && typeof b === "string") return compareCodePoints(a, b)
  if (ra === 2) return integerValue(a) - integerValue(b)
  return 0
}

function compareKeys(
  a: readonly (JsonValue | undefined)[],
  b: readonly (JsonValue | undefined)[],
): number {
  for (let i = 0; i < a.length; i += 1) {
    const result = compareKeyValue(a[i], b[i])
    if (result !== 0) return result
  }
  return 0
}

/** Rows must be strictly increasing by their key fields before any positional join. */
function orderedRows(rows: readonly Row[], fields: readonly string[], detail: string): void {
  const keys = rows.map((row) => fields.map((field) => row[field]))
  for (let i = 1; i < keys.length; i += 1) {
    if (compareKeys(keys[i - 1] ?? [], keys[i] ?? []) >= 0)
      fail("rows-unsorted-or-duplicate", detail)
  }
}

export function validateFragments(fragments: readonly Fragment[]): void {
  for (const fragment of fragments) {
    const fields = fragment.value["base"] === null ? primaryKey(fragment.table) : ["row_index"]
    orderedRows(
      fragment.rows,
      fields,
      `rows of ${fragment.file}/${fragment.table} must be sorted with unique keys`,
    )
  }
  const groups = new Map<string, Fragment[]>()
  for (const fragment of fragments) {
    const key = `${fragment.file}\u0000${fragment.table}`
    groups.set(key, [...(groups.get(key) ?? []), fragment])
  }
  for (const [group, members] of groups) {
    const keys = members.map((f) => {
      const owner = objectValue(f.value["owner"])
      return [owner["kind"], owner["id"], f.value["bucket"], f.value["partition"]]
    })
    for (let i = 1; i < keys.length; i += 1) {
      if (compareKeys(keys[i - 1] ?? [], keys[i] ?? []) > 0)
        fail("fragment-order", `fragments out of order in ${group.replace("\u0000", "/")}`)
    }
  }
}

function parameterSchema(value: Row): void {
  const params = arrayValue(value["parameters"]).map((item) => objectValue(item))
  orderedRows(params, ["name"], "parameters must be sorted with unique names")
  for (const item of params) {
    const limits = item["uint"]
    if (limits !== null && limits !== undefined) {
      const range = objectValue(limits)
      if (integerValue(range["minimum"]) > integerValue(range["maximum"]))
        fail("parameter-range-inverted", "inverted parameter range")
    }
  }
}

function spellings(symbol: Row): void {
  const params = new Map<string, Row>()
  for (const item of arrayValue(objectValue(symbol["parameter_schema"])["parameters"])) {
    const parameter = objectValue(item)
    params.set(stringValue(parameter["name"]), parameter)
  }
  for (const raw of arrayValue(symbol["spellings"])) {
    const spelling = objectValue(raw)
    const kind = spelling["parse_kind"]
    if (kind === "literal") continue
    const parameter = params.get(stringValue(spelling["parameter_name"]))
    if (!parameter)
      fail("spelling-undeclared-parameter", "spelling references an undeclared parameter")
    if (
      (kind === "uint" && parameter["uint"] === null) ||
      (kind === "variable" && arrayValue(parameter["variables"]).length === 0)
    ) {
      fail("spelling-disabled-domain", "spelling requires an enabled parameter domain")
    }
  }
}

function hints(ruling: Row): void {
  const schemas = new Set(
    arrayValue(ruling["hints"]).map((hint) =>
      canonicalText(objectValue(hint)["parameter_schema"] ?? null),
    ),
  )
  if (schemas.size > 1)
    fail("hints-parameters-differ", "multilingual hints must declare identical parameters")
}

const NESTED_ORDER: readonly (readonly [string, readonly string[]])[] = [
  ["translations", ["field", "ordinal", "target_lang"]],
  ["sections", ["ordinal"]],
  ["regions", ["region"]],
  ["current", ["region"]],
  ["overrides", ["region"]],
  ["region_blocks", ["region"]],
]

function nested(value: JsonValue): void {
  if (Array.isArray(value)) {
    for (const item of value) nested(item)
    return
  }
  if (!isObject(value)) return
  if ("parameters" in value) parameterSchema(value)
  for (const [name, keys] of NESTED_ORDER) {
    const list = value[name]
    if (Array.isArray(list) && list.every(isObject))
      orderedRows(list, keys, `${name} must be sorted with unique keys`)
  }
  for (const item of Object.values(value)) nested(item)
}

const VOCABULARY_FIELDS: Readonly<Record<string, string>> = {
  class_code: "class",
  type_code: "type",
  rarity_code: "rarity",
  frame_code: "frame",
  series_code: "stamp_series",
  traits: "trait",
  titles: "title",
  special_kinds: "special_kind",
}

function vocabulary(view: View): void {
  const vocab = new Set(
    (view["vocabulary"] ?? []).map(
      (row) => `${stringValue(row["kind"])}\u0000${stringValue(row["code"])}`,
    ),
  )
  const check = (value: JsonValue): void => {
    if (Array.isArray(value)) {
      for (const item of value) check(item)
      return
    }
    if (!isObject(value)) return
    for (const [field, item] of Object.entries(value)) {
      const kind = VOCABULARY_FIELDS[field]
      if (kind !== undefined && item !== null) {
        const codes = Array.isArray(item) ? item : [item]
        if (codes.some((code) => !vocab.has(`${kind}\u0000${stringValue(code)}`)))
          fail("vocabulary-missing", `vocabulary reference missing for ${field}`)
      }
      check(item)
    }
  }
  for (const table of ["face_revision", "printing", "stamp"])
    for (const row of view[table] ?? []) check(row)
}

function ownerCard(
  table: string,
  row: Row,
  faces: Map<string, Row>,
  arts: Map<string, Row>,
): JsonValue | undefined {
  if (table === "card") return row["id"]
  if (table === "face_revision") return faces.get(stringValue(row["face_id"]))?.["card_id"]
  if (table === "digital_art_link") return arts.get(stringValue(row["art_id"]))?.["card_id"]
  return row["card_id"] ?? row["from_card_id"]
}

function owners(view: View, fragments: readonly Fragment[]): void {
  const cards = new Map((view["card"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const faces = new Map((view["face"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const arts = new Map((view["art"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const families = new Set(
    (view["product_family"] ?? []).map((row) => canonicalText(row["id"] ?? null)),
  )
  const printOwners = new Map<string, string>()
  for (const fragment of fragments) {
    const owner = objectValue(fragment.value["owner"])
    const home =
      CARD_TABLES.has(fragment.table) ||
      PRINT_TABLES.has(fragment.table) ||
      ART_TABLES.has(fragment.table)
    if (
      owner["kind"] !== (home ? "home_set" : "global") ||
      (home && !families.has(canonicalText(owner["id"] ?? null)))
    ) {
      fail("owner-mismatch", `incorrect owner kind or family for ${fragment.table}`)
    }
    if (fragment.value["base"] !== null) continue
    const ownerId = canonicalText(owner["id"] ?? null)
    for (const row of fragment.rows) {
      if (PRINT_TABLES.has(fragment.table)) {
        const key = stringValue(row[fragment.table === "printing" ? "id" : "printing_id"])
        const previous = printOwners.get(key)
        if (previous !== undefined && previous !== ownerId)
          fail("printing-owner-conflict", "printing fragments disagree on owner")
        printOwners.set(key, ownerId)
      } else if (CARD_TABLES.has(fragment.table) || ART_TABLES.has(fragment.table)) {
        const cardId = ownerCard(fragment.table, row, faces, arts)
        const card = cards.get(stringValue(cardId))
        if (!card || canonicalText(card["home_set_id"] ?? null) !== ownerId)
          fail("owner-mismatch", `owner does not match card home_set in ${fragment.table}`)
      }
    }
  }
}

function summaries(view: View, manifest: Row): void {
  const versions = new Map((view["qa_version"] ?? []).map((row) => [stringValue(row["id"]), row]))
  const qa = new Set<string>()
  for (const row of view["qa"] ?? []) {
    const versionId = row["current_version_id"]
    if (versionId === null || versionId === undefined) continue
    for (const card of arrayValue(versions.get(stringValue(versionId))?.["cards"]))
      qa.add(stringValue(card))
  }
  const faces = new Map(
    (view["face"] ?? []).map((row) => [stringValue(row["id"]), stringValue(row["card_id"])]),
  )
  const prints = new Map(
    (view["printing"] ?? []).map((row) => [stringValue(row["id"]), stringValue(row["card_id"])]),
  )
  const errata = new Set<string>()
  for (const row of view["errata"] ?? []) {
    for (const raw of arrayValue(row["versions"])) {
      const version = objectValue(raw)
      for (const item of arrayValue(version["changes"])) {
        const card = faces.get(stringValue(objectValue(item)["face_id"]))
        if (card === undefined)
          fail("dangling-reference", "errata change references a missing face")
        errata.add(card)
      }
      for (const item of arrayValue(version["printings"])) {
        const card = prints.get(stringValue(objectValue(item)["printing_id"]))
        if (card === undefined)
          fail("dangling-reference", "errata printing references a missing printing")
        errata.add(card)
      }
    }
  }
  const sorted = (set: Set<string>): string[] => [...set].sort(compareCodePoints)
  if (
    canonicalText(manifest["qa_card_ids"] ?? null) !== canonicalText(sorted(qa)) ||
    canonicalText(manifest["errata_card_ids"] ?? null) !== canonicalText(sorted(errata))
  ) {
    fail("summary-mismatch", "QA/errata summary mismatch")
  }
  const universe: JsonValue[] = (view["keyword"] ?? []).map((row) => ({
    id: row["id"] ?? null,
    kind: row["kind"] ?? null,
    definition_unit_id: row["definition_unit_id"] ?? null,
    actions: row["actions"] ?? null,
  }))
  if (digest(utf8(canonicalText(universe))) !== manifest["mechanic_universe_id"])
    fail("mechanic-universe-mismatch", "mechanic universe hash mismatch")
}

/** Public relationships that need the joined view and the manifest, without any build DB. */
export function validateView(view: View, manifest: Row, fragments: readonly Fragment[]): void {
  owners(view, fragments)
  vocabulary(view)
  summaries(view, manifest)
  for (const rows of Object.values(view)) for (const row of rows) nested(row)
  for (const symbol of view["text_symbol"] ?? []) spellings(symbol)
  for (const ruling of view["ruling_revision"] ?? []) hints(ruling)
  const supported = new Set(
    (view["card_engine_support"] ?? []).map((row) => stringValue(row["card_id"])),
  )
  const cardIds = new Set((view["card"] ?? []).map((row) => stringValue(row["id"])))
  if (supported.size !== cardIds.size || [...cardIds].some((id) => !supported.has(id)))
    fail("support-missing", "every card requires support")
  const images = new Map((view["image_asset"] ?? []).map((row) => [stringValue(row["id"]), row]))
  for (const row of view["image_variant"] ?? []) {
    const asset = images.get(stringValue(row["image_id"]))
    if (
      !asset ||
      asset["publication_state"] !== "approved" ||
      asset["availability"] !== "available"
    ) {
      fail("image-variant-unapproved", "variant requires an approved available image")
    }
  }
  const printings = new Map((view["printing"] ?? []).map((row) => [stringValue(row["id"]), row]))
  for (const row of view["printing_image"] ?? []) {
    const printing = printings.get(stringValue(row["printing_id"]))
    const faceIds = arrayValue(printing?.["faces"]).map(
      (face) => objectValue(face)["face_id"] ?? null,
    )
    if (!faceIds.some((id) => canonicalText(id) === canonicalText(row["face_id"] ?? null)))
      fail("printing-image-face", "image references an absent printing face")
  }
}

const TEMPLATE_PARAMETERS: readonly (readonly [string, string, readonly string[]])[] = [
  ["digital_endpoints", "card_url_template", ["official_id", "provider_lang"]],
  ["shop_links", "url_template", ["card_no", "region"]],
]

/** Language fallback closure and the finite URL template parameter lists. */
export function validateConfig(config: Row): void {
  const langs = arrayValue(config["languages"]).map((item) => objectValue(item))
  orderedRows(langs, ["code"], "languages must be sorted with unique codes")
  const codes = new Set(langs.map((lang) => stringValue(lang["code"])))
  for (const lang of langs) {
    const fallback = arrayValue(lang["fallback_order"]).map((item) => stringValue(item))
    if (fallback.includes(stringValue(lang["code"])) || fallback.some((code) => !codes.has(code)))
      fail("config-language-fallback", "invalid language fallback")
  }
  for (const [section, field, allowed] of TEMPLATE_PARAMETERS) {
    for (const raw of arrayValue(config[section])) {
      let template = stringValue(objectValue(raw)[field])
      for (const name of allowed) template = template.replaceAll(`{${name}}`, "example")
      let url: URL
      try {
        url = new URL(template)
      } catch {
        fail("config-url-template", "invalid public URL template")
      }
      if (url.username !== "" || url.password !== "")
        fail("config-url-template", "public URL cannot contain credentials")
      if (
        template.includes("{") ||
        template.includes("}") ||
        url.protocol !== "https:" ||
        url.hostname === ""
      )
        fail("config-url-template", "invalid public URL template")
    }
  }
}
