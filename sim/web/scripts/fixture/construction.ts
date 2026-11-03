// The construction index carddb writes next to a snapshot (docs/schema/export/construction-index.md),
// projected from the fixture's own rows so the deck builder has rules data without a real export.
// Synthetic cards print no copy-changing text, so `copies` and `singleton_except` stay empty.

import {
  arrayValue,
  canonical,
  compareCodePoints,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "../../src/data/format-v3/json"

const ZONES = ["none", "leader", "main", "evolve"]
const ZONE_OF_ROLE: Readonly<Record<string, number>> = { leader: 1, main: 2, evolve: 3 }

export function constructionIndexFiles(
  rowsOf: (table: string) => readonly JsonObject[],
  manifest: JsonObject,
  manifestHash: string,
): Map<string, Uint8Array> {
  const files = new Map<string, Uint8Array>()
  for (const region of arrayValue(manifest["regions"]).map((value) => stringValue(value))) {
    const index = projectRegion(rowsOf, manifest, manifestHash, region)
    files.set(`construction/${manifestHash.slice(7)}/${region}.json`, canonical(index))
  }
  return files
}

function projectRegion(
  rowsOf: (table: string) => readonly JsonObject[],
  manifest: JsonObject,
  manifestHash: string,
  region: string,
): JsonObject {
  const vocabulary = (kind: string) =>
    rowsOf("vocabulary")
      .filter((row) => row["kind"] === kind)
      .map((row) => stringValue(row["code"]))
      .sort(compareCodePoints)
  // Synthetic neutral cards carry no class; real exports give them the `neutral` code.
  const classes = [...vocabulary("class"), "neutral"].sort(compareCodePoints)
  const titles = vocabulary("title")
  const names = rowsOf("rules_name")
    .filter((row) => row["region"] === region)
    .map((row) => stringValue(row["id"]))
    .sort(compareCodePoints)
  const nameCode = new Map(names.map((id, code) => [id, code]))
  const faceName = new Map(
    rowsOf("face_rules_name")
      .filter((row) => row["region"] === region && row["role"] === "primary")
      .map((row) => [stringValue(row["face_id"]), nameCode.get(stringValue(row["rules_name_id"]))]),
  )
  const faces = new Map(rowsOf("face").map((row) => [stringValue(row["id"]), row]))
  const revisions = new Map(rowsOf("face_revision").map((row) => [stringValue(row["id"]), row]))

  const released = rowsOf("card")
    .flatMap((card) => {
      const view = arrayValue(card["regions"])
        .map((value) => objectValue(value))
        .find((row) => row["region"] === region && row["release_state"] === "released")
      return view ? [{ card, view }] : []
    })
    .sort((a, b) => compareCodePoints(stringValue(a.card["id"]), stringValue(b.card["id"])))
  const columns: Record<string, JsonValue[]> = { id: [], name: [], class: [], titles: [], zone: [] }
  for (const { card, view } of released) {
    const front = faces.get(stringValue(arrayValue(card["faces"])[0]))
    const current = arrayValue(front?.["current"] ?? [])
      .map((value) => objectValue(value))
      .find((row) => row["region"] === region)
    const revision = revisions.get(stringValue(current?.["revision_id"]))
    const classCode = revision?.["class_code"]
    columns["id"]?.push(card["id"] ?? null)
    columns["name"]?.push(faceName.get(stringValue(front?.["id"])) ?? null)
    const zone = ZONE_OF_ROLE[stringValue(view["deck_role"] ?? "")] ?? 0
    const effective = typeof classCode === "string" ? classCode : zone === 0 ? null : "neutral"
    columns["class"]?.push(effective === null ? -1 : classes.indexOf(effective))
    columns["titles"]?.push(
      arrayValue(revision?.["titles"] ?? []).reduce<number>(
        (bits, title) => bits | (1 << titles.indexOf(stringValue(title))),
        0,
      ),
    )
    columns["zone"]?.push(zone)
  }

  const formats: JsonObject = {}
  for (const profile of rowsOf("rules_profile").filter((row) => row["region"] === region)) {
    const coverage = arrayValue(manifest["restriction_coverage"])
      .map((value) => objectValue(value))
      .filter((row) => row["profile_id"] === profile["id"] && row["state"] === "complete")
      .map((row) => [row["from_date"] ?? null, row["until_date"] ?? null])
    const restrictions = rowsOf("restriction")
      .filter((row) => row["profile_id"] === profile["id"] && row["state"] === "confirmed")
      .sort((a, b) => compareCodePoints(stringValue(a["id"]), stringValue(b["id"])))
      .map((row) => ({
        id: row["id"] ?? null,
        kind: row["kind"] ?? null,
        from: row["effective_from"] ?? null,
        until: row["effective_until"] ?? null,
        max: row["max_copies"] ?? null,
        max_groups: row["max_selected_groups"] ?? null,
        members: arrayValue(row["members"]).map((value) => {
          const member = objectValue(value)
          return [
            nameCode.get(stringValue(member["rules_name_id"])) ?? null,
            member["deck_scope"] ?? null,
            member["choice_option"] ?? null,
          ]
        }),
      }))
    formats[stringValue(profile["format_code"])] = { coverage, restrictions }
  }
  return {
    construction_index_format: 1,
    region,
    snapshot: { data_version: manifest["data_version"] ?? null, manifest_sha256: manifestHash },
    classes,
    titles,
    title_rules: titles.includes("cardfight_vanguard") ? { cardfight_vanguard: "vanguard" } : {},
    zones: ZONES,
    names: names.length,
    cards: columns,
    copies: [],
    singleton_except: [],
    start_amulet: [],
    formats,
  }
}
