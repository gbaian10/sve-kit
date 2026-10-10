import type { Row } from "../format-v3/decode"
import { integerValue, objectValue, stringValue } from "../format-v3/json"
import type { Fragment } from "../format-v3/reader"
import type { CardIndex } from "../store"
import { Columns, StringPool } from "./columns"

const TABLES = new Set([
  "card",
  "face",
  "printing",
  "face_revision",
  "product_family",
  "vocabulary",
  "text_unit",
  "translation",
  "search_alias",
])

const FIELDS: Readonly<Record<string, readonly string[]>> = {
  card: ["id", "home_set_id", "regions"],
  face: ["id", "card_id", "ordinal", "current", "wording"],
  printing: ["id", "card_id", "region", "card_no", "int_id", "home_set_id"],
  face_revision: [
    "id",
    "face_id",
    "class_code",
    "type_code",
    "cost",
    "attack",
    "defense",
    "traits",
    "name_unit_id",
    "translations",
  ],
  text_unit: ["id", "lang", "text"],
  translation: ["id", "text_unit_id", "low_confidence"],
  search_alias: ["kind", "code", "lang", "text"],
}

function project(table: string, row: Row): Row {
  const fields = FIELDS[table]
  const projected = fields
    ? Object.fromEntries(
        fields.filter((field) => field in row).map((field) => [field, row[field] ?? null]),
      )
    : row
  if (table === "card")
    projected["regions"] = ((row["regions"] ?? []) as Row[]).map((region) => ({
      region: region["region"] ?? null,
      default_printing_id: region["default_printing_id"] ?? null,
    }))
  if (table === "face_revision")
    projected["translations"] = ((row["translations"] ?? []) as Row[]).filter(
      (translation) => translation["field"] === "name",
    )
  if (table === "face")
    projected["wording"] = ((row["wording"] ?? []) as Row[]).map((wording) => ({
      region: wording["region"] ?? null,
      state: wording["state"] ?? null,
      display: wording["display"] ?? null,
    }))
  return projected
}

/** Only search columns survive ingestion; lookup maps contain ordinals instead of decoded rows. */
export class BootstrapColumns {
  readonly pool = new StringPool(false)
  private readonly tables = new Map<string, Columns>()

  ingest(fragments: readonly Fragment[]): void {
    // One block per table and file: a fragment often holds only a couple of rows, and a block
    // per fragment costs more in typed-array headers than the rows themselves.
    const tables = new Map<string, Row[]>()
    for (const fragment of fragments) {
      if (!TABLES.has(fragment.table)) continue
      const rows = tables.get(fragment.table) ?? []
      tables.set(fragment.table, rows)
      for (const row of fragment.rows)
        rows.push(
          project(
            fragment.table,
            fragment.table === "printing"
              ? { ...row, home_set_id: objectValue(fragment.value["owner"])["id"] ?? null }
              : row,
          ),
        )
    }
    for (const [name, rows] of tables) {
      let table = this.tables.get(name)
      if (!table) {
        table = new Columns(this.pool)
        this.tables.set(name, table)
      }
      table.append(rows)
    }
  }

  seal(): void {
    this.pool.seal()
  }

  *cards(): Generator<Row> {
    yield* this.tables.get("card")?.iterate() ?? []
  }

  allocation(): { readonly arrayBuffers: number; readonly stringBytes: number } {
    return {
      arrayBuffers:
        this.pool.bufferBytes() + [...this.tables.values()].reduce((n, t) => n + t.bytes(), 0),
      stringBytes: this.pool.bytes(),
    }
  }

  rows = (name: string): Row[] => this.tables.get(name)?.rows() ?? []

  index(): CardIndex {
    const table = (name: string) => this.tables.get(name)
    const maps = new Map<string, Map<string, number>>()
    const groups = new Map<string, Map<string, number[]>>()
    for (const name of TABLES) {
      const values = table(name)
      const map = new Map<string, number>()
      const grouped = new Map<string, number[]>()
      for (let ordinal = 0; ordinal < (values?.length ?? 0); ordinal += 1) {
        const id = values?.value(ordinal, "id")
        if (typeof id === "string") map.set(id, ordinal)
        if (name === "vocabulary")
          map.set(
            `${stringValue(values?.value(ordinal, "kind"))}\u0000${stringValue(values?.value(ordinal, "code"))}`,
            ordinal,
          )
        const card = values?.value(ordinal, "card_id")
        if (typeof card === "string") {
          const list = grouped.get(card) ?? []
          list.push(ordinal)
          grouped.set(card, list)
        }
      }
      maps.set(name, map)
      groups.set(name, grouped)
    }
    const get = (name: string, id: string): Row | undefined => {
      const ordinal = maps.get(name)?.get(id)
      return ordinal === undefined ? undefined : table(name)?.row(ordinal)
    }
    const group = (name: string, cardId: string): Row[] =>
      (groups.get(name)?.get(cardId) ?? []).map((ordinal) => table(name)?.row(ordinal) ?? {})
    const currentRevision = (faceId: string, region: string) => {
      const current = (get("face", faceId)?.["current"] ?? []) as Row[]
      const ref = current.find((row) => row["region"] === region)
      return ref ? get("face_revision", stringValue(ref["revision_id"])) : undefined
    }
    const wording = (faceId: string, region: string) =>
      ((get("face", faceId)?.["wording"] ?? []) as Row[]).find((row) => row["region"] === region)
    return {
      get cards() {
        return table("card")?.rows() ?? []
      },
      get families() {
        return table("product_family")?.rows() ?? []
      },
      card: (id) => get("card", id),
      face: (id) => get("face", id),
      facesOf: (id) =>
        group("face", id).sort((a, b) => integerValue(a["ordinal"]) - integerValue(b["ordinal"])),
      printing: (id) => get("printing", id),
      printingsOf: (id) => group("printing", id),
      printingByCardNo: () => {
        throw new Error("printingByCardNo is unavailable in search staging")
      },
      currentRevision,
      wording,
      displayRevision: (faceId, region) => {
        const current = currentRevision(faceId, region)
        if (current) return current
        const display = wording(faceId, region)?.["display"] as Row | undefined
        return typeof display?.["revision_id"] === "string"
          ? get("face_revision", display["revision_id"])
          : undefined
      },
      family: (id) => get("product_family", id),
      product: () => {
        throw new Error("product is unavailable in search staging")
      },
      vocabulary: (kind, code) => get("vocabulary", `${kind}\u0000${code}`),
      keyword: () => {
        throw new Error("keyword is unavailable in search staging")
      },
      support: () => {
        throw new Error("support is unavailable in search staging")
      },
      textUnit: (id) => get("text_unit", id),
      translation: (id) => get("translation", id),
    }
  }
}
