import type { Region } from "../../domain/search"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  objectValue,
  stringValue,
} from "../format-v3/json"
import { type Files, findBase, type Fragment, joinDetail, mergedRows } from "../format-v3/reader"

interface Projection {
  readonly card: string
  readonly region: Region
  readonly revisions: readonly JsonObject[]
  readonly stats: string
  readonly names: readonly string[]
}

function normalize(text: string, names: readonly string[]): string {
  let value = text.normalize("NFKC").toLowerCase()
  for (const name of [...new Set(names)].sort((a, b) => b.length - a.length)) {
    const normalized = name.normalize("NFKC").toLowerCase()
    if (!normalized) continue
    const escaped = normalized.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&").replace(/\s+/gu, "\\s*")
    // Keep word boundaries even for Japanese particles to avoid replacing part of another card's name.
    const self = new RegExp(`(?<![\\p{L}\\p{N}_])${escaped}(?![\\p{L}\\p{N}_])`, "gu")
    value = value.replace(self, "\0self\0")
  }
  value = value
    .replace(/\s+/gu, "")
    .replace(/[\u2010-\u2015\u2212]/gu, "-")
    .replace(/[\u2018\u2019]/gu, "'")
  const characters = Array.from(value)
  return characters
    .filter((character, ordinal) => {
      if (!/\p{P}/u.test(character) || /[-/\\]/u.test(character)) return true
      if (
        character === "'" &&
        /[\p{L}\p{N}]/u.test(characters[ordinal - 1] ?? "") &&
        /[\p{L}\p{N}]/u.test(characters[ordinal + 1] ?? "")
      )
        return true
      // Numeric punctuation and operators must not turn 1.5 or -1 into a different quantity.
      return (
        /[.,]/u.test(character) &&
        /\d/u.test(characters[ordinal - 1] ?? "") &&
        /\d/u.test(characters[ordinal + 1] ?? "")
      )
    })
    .join("")
}

function excludedKind(row: JsonObject): boolean {
  return (
    ["ep", "sep"].includes(stringValue(row["type_code"])) ||
    arrayValue(row["special_kinds"]).includes("token")
  )
}

/** Comparing rules never merges permanent identities or infers a cross-region correspondence. */
export class PrOnlyRules {
  readonly printings: readonly JsonObject[]
  private readonly catalog: readonly Fragment[]
  private readonly comparisons: { own: Projection; peers: readonly Projection[] }[] = []
  private readonly ineligible = new Set<string>()
  private readonly bases = new Set<string>()

  constructor(catalog: readonly Fragment[]) {
    this.catalog = catalog
    const pr = new Set(
      mergedRows(catalog, "product_family")
        .filter((row) => row["public_code"] === "PR")
        .map((row) => stringValue(row["id"])),
    )
    const printings = new Map<string, JsonObject>()
    for (const fragment of catalog) {
      if (!["printing", "card_locator"].includes(fragment.table)) continue
      for (const row of fragment.rows)
        printings.set(stringValue(row["id"]), {
          card_id: row["card_id"] ?? null,
          card_no: row["card_no"] ?? null,
          region: row["region"] ?? null,
          home_set_id:
            fragment.table === "card_locator"
              ? (row["home_set_id"] ?? null)
              : (objectValue(fragment.value["owner"])["id"] ?? null),
        })
    }
    this.printings = [...printings.values()]
    const regular = new Set(
      this.printings
        .filter(
          (row) => !pr.has(stringValue(row["home_set_id"])) || !stringValue(row["card_no"]).trim(),
        )
        .map((row) => stringValue(row["card_id"])),
    )
    const candidates = new Set(
      this.printings.map((row) => stringValue(row["card_id"])).filter((card) => !regular.has(card)),
    )
    const faces = new Map<string, JsonObject[]>()
    for (const face of mergedRows(catalog, "face")) {
      const card = stringValue(face["card_id"]),
        rows = faces.get(card) ?? []
      rows.push(face)
      faces.set(card, rows)
    }
    const revisions = new Map(
      mergedRows(catalog, "face_revision").map((row) => [stringValue(row["id"]), row]),
    )
    const texts = new Map(
      mergedRows(catalog, "text_unit").map((row) => [
        stringValue(row["id"]),
        stringValue(row["text"]),
      ]),
    )
    const projections: Projection[] = []
    const regions = new Map<string, Set<Region>>()
    for (const printing of this.printings) {
      const card = stringValue(printing["card_id"]),
        region = printing["region"]
      if (region !== "jp" && region !== "en") continue
      const values = regions.get(card) ?? new Set<Region>()
      values.add(region)
      regions.set(card, values)
    }
    for (const [card, editions] of regions) {
      for (const region of editions) {
        const selected = (faces.get(card) ?? [])
          .toSorted((a, b) => Number(a["ordinal"]) - Number(b["ordinal"]))
          .map((face) => {
            const current = arrayValue(face["current"])
              .map((value) => objectValue(value))
              .find((row) => row["region"] === region)
            const wording = arrayValue(face["wording"] ?? [])
              .map((value) => objectValue(value))
              .find((row) => row["region"] === region)
            const ref =
              current ?? (wording?.["display"] ? objectValue(wording["display"]) : undefined)
            return typeof ref?.["revision_id"] === "string"
              ? revisions.get(ref["revision_id"])
              : undefined
          })
        if (!selected.length || selected.some((row) => row === undefined)) {
          if (candidates.has(card)) this.ineligible.add(card)
          continue
        }
        const rows = selected.filter((row): row is JsonObject => row !== undefined)
        if (rows.some(excludedKind)) {
          if (candidates.has(card)) this.ineligible.add(card)
          continue
        }
        const names = rows.map((row) => texts.get(stringValue(row["name_unit_id"])) ?? "")
        projections.push({
          card,
          region,
          revisions: rows,
          names,
          stats: canonicalText(
            rows.map((row) => [
              row["type_code"] ?? null,
              row["cost"] ?? null,
              row["attack"] ?? null,
              row["defense"] ?? null,
              arrayValue(row["traits"])
                .map((value) => stringValue(value))
                .sort(),
              arrayValue(row["special_kinds"])
                .map((value) => stringValue(value))
                .sort(),
            ]),
          ),
        })
      }
    }
    const peers = new Map<string, Projection[]>()
    for (const projection of projections) {
      if (!regular.has(projection.card)) continue
      const key = `${projection.region}/${projection.stats}`,
        group = peers.get(key) ?? []
      group.push(projection)
      peers.set(key, group)
    }
    const required = new Set<string>()
    for (const own of projections) {
      if (!candidates.has(own.card) || this.ineligible.has(own.card)) continue
      const matches = peers.get(`${own.region}/${own.stats}`) ?? []
      if (!matches.length) continue
      this.comparisons.push({ own, peers: matches })
      for (const projection of [own, ...matches])
        for (const revision of projection.revisions) required.add(stringValue(revision["id"]))
    }
    for (const fragment of catalog)
      if (
        fragment.table === "face_revision" &&
        fragment.rows.some((row) => required.has(stringValue(row["id"])))
      )
        this.bases.add(fragment.file)
  }

  /** Only matching structural peers need their full rules; ordinary searches download none. */
  details(files: Files): readonly JsonObject[] {
    const selected = new Set<string>()
    const visit = (key: string) => {
      const file = files.get(key)
      if (!file) throw new Error("PR rules dependency missing")
      // Dependencies may carry the original dictionary alongside translated text.
      if (file["purpose"] === "catalog" || selected.has(key)) return
      selected.add(key)
      for (const dependency of arrayValue(file["dependencies"]))
        visit(stringValue(objectValue(dependency)["key"]))
    }
    for (const [key, file] of files)
      if (
        file["purpose"] === "detail" &&
        arrayValue(file["row_counts"]).some((count) => arrayValue(count)[0] === "face_revision") &&
        arrayValue(file["dependencies"]).some((ref) =>
          this.bases.has(stringValue(objectValue(ref)["key"])),
        )
      )
        visit(key)
    return [...selected].flatMap((key) => {
      const file = files.get(key)
      return file ? [file] : []
    })
  }

  excluded(details: readonly Fragment[]): ReadonlySet<string> {
    const fragments = [...this.catalog, ...details]
    const texts = new Map<string, string>()
    const revisions = new Map<string, JsonObject>()
    for (const fragment of fragments) {
      if (fragment.table === "text_unit")
        for (const row of fragment.rows) texts.set(stringValue(row["id"]), stringValue(row["text"]))
      if (fragment.table !== "face_revision") continue
      const rows =
        fragment.value["base"] === null
          ? fragment.rows
          : joinDetail(fragment, findBase(fragment, fragments), new Map())
      for (const row of rows) revisions.set(stringValue(row["id"]), row)
    }
    const signatures = new Map<Projection, string | undefined>()
    const signature = (projection: Projection): string | undefined => {
      if (signatures.has(projection)) return signatures.get(projection)
      const values = []
      for (const source of projection.revisions) {
        const row = revisions.get(stringValue(source["id"]))
        const body =
          typeof row?.["effect_unit_id"] === "string" ? texts.get(row["effect_unit_id"]) : undefined
        if (!row || body === undefined) {
          signatures.set(projection, undefined)
          return undefined
        }
        const sections = arrayValue(row["sections"] ?? [])
          .map((value) => objectValue(value))
          .toSorted((a, b) => Number(a["ordinal"]) - Number(b["ordinal"]))
        if (sections.some((section) => !texts.has(stringValue(section["text_unit_id"])))) {
          signatures.set(projection, undefined)
          return undefined
        }
        values.push([
          normalize(body, projection.names),
          sections.map((section) => [
            section["kind"] ?? null,
            normalize(texts.get(stringValue(section["text_unit_id"])) ?? "", projection.names),
          ]),
        ])
      }
      const result = canonicalText(values)
      signatures.set(projection, result)
      return result
    }
    const excluded = new Set(this.ineligible)
    for (const { own, peers } of this.comparisons) {
      const value = signature(own)
      if (
        value === undefined ||
        peers.some((peer) => {
          const other = signature(peer)
          return other === undefined || other === value
        })
      )
        excluded.add(own.card)
    }
    return excluded
  }
}
