import type { Region } from "../domain/search"
import type { Row } from "./format-v3/decode"
import { arrayValue, type JsonObject, objectValue, stringValue } from "./format-v3/json"
import type { CardIndex } from "./store"

export interface RestrictionStatus {
  readonly state: "unknown" | "partial" | "complete"
  readonly onDate: string
  readonly synthetic?: boolean
}
export interface RestrictionSelection {
  readonly status: RestrictionStatus
  readonly cards: ReadonlySet<string>
}

const contains = (row: Row, day: string, from: string, until: string) =>
  typeof row[from] === "string" &&
  row[from] <= day &&
  (row[until] === null || (typeof row[until] === "string" && day < row[until]))

/** Reads explicit snapshot dates; a missing coverage window never proves absence. */
export function selectRestrictions(
  manifest: JsonObject,
  rows: (table: string) => readonly Row[],
  index: CardIndex,
  edition: Region,
  onDate: string,
): RestrictionSelection {
  const cards = new Set<string>()
  const unknown = { status: { state: "unknown", onDate }, cards } as const
  if (onDate > stringValue(manifest["data_as_of"])) return unknown
  const profiles = rows("rules_profile").filter(
    (row) => row["region"] === edition && row["format_code"] === "standard",
  )
  if (profiles.length !== 1) return unknown
  const profile = profiles[0]
  const revisions = arrayValue(profile?.["revisions"] ?? [])
    .map((value) => objectValue(value))
    .filter((row) => contains(row, onDate, "effective_from", "effective_until"))
  if (revisions.length !== 1) return unknown
  const windows = arrayValue(manifest["restriction_coverage"])
    .map((value) => objectValue(value))
    .filter(
      (row) =>
        row["profile_id"] === profile?.["id"] && contains(row, onDate, "from_date", "until_date"),
    )
  if (windows.length !== 1) return unknown
  const names = new Map(
    rows("rules_name")
      .filter((row) => row["region"] === edition)
      .map((row) => [row["id"], row]),
  )
  const bindings = rows("face_rules_name").filter((row) => names.has(row["rules_name_id"]))
  const active = rows("restriction").filter(
    (row) =>
      row["profile_id"] === profile?.["id"] &&
      contains(row, onDate, "effective_from", "effective_until"),
  )
  let incomplete = windows[0]?.["state"] !== "complete"
  for (const restriction of active) {
    if (restriction["state"] === "withdrawn") continue
    if (restriction["state"] !== "confirmed") {
      incomplete = true
      continue
    }
    if (!["copy_limit", "choice_group"].includes(stringValue(restriction["kind"]))) {
      incomplete = true
      continue
    }
    const limit = restriction["max_copies"]
    const baseline = revisions[0]?.["default_copy_limit"]
    if (
      restriction["kind"] === "copy_limit" &&
      (typeof limit !== "number" ||
        (limit !== 0 && (typeof baseline !== "number" || limit >= baseline)))
    ) {
      incomplete = true
      continue
    }
    const members = arrayValue(restriction["members"]).map((value) => objectValue(value))
    if (!members.length) incomplete = true
    for (const member of members) {
      const matching = bindings.filter(
        (binding) => binding["rules_name_id"] === member["rules_name_id"],
      )
      if (!matching.length) incomplete = true
      for (const binding of matching) {
        const face = index.face(stringValue(binding["face_id"]))
        if (!face) {
          incomplete = true
          continue
        }
        const card = index.card(stringValue(face["card_id"]))
        const region = arrayValue(card?.["regions"] ?? [])
          .map((value) => objectValue(value))
          .find((row) => row["region"] === edition)
        const scope = member["deck_scope"]
        if (scope === "all" || region?.["deck_role"] === scope)
          cards.add(stringValue(face["card_id"]))
      }
    }
  }
  return {
    status: {
      state: incomplete ? "partial" : "complete",
      onDate,
      synthetic: windows[0]?.["source_url"] === "https://example.invalid/restrictions",
    },
    cards,
  }
}
