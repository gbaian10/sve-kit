import type { Region } from "../domain/search"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { Row } from "./format-v3/decode"
import { arrayValue, type JsonObject, objectValue, stringValue } from "./format-v3/json"
import { type Files, mergedRows } from "./format-v3/reader"
import { type CardIndex, createCardIndex } from "./store"

export interface RestrictionStatus {
  readonly state: "unknown" | "partial" | "complete"
  readonly onDate: string
  readonly synthetic?: boolean
}
export interface CardRestriction {
  readonly id: string
  readonly formatCode: "standard"
  readonly maxCopies: number | null
  readonly effectiveFrom: string
  readonly sourceUrl: string | null
  readonly asOf: string
}
export interface CardRestrictionDetails {
  readonly status: RestrictionStatus
  readonly restrictions: readonly CardRestriction[]
}
export interface RestrictionSelection {
  readonly details: ReadonlyMap<string, readonly CardRestriction[]>
  readonly status: RestrictionStatus
  readonly cards: ReadonlySet<string>
}

export const RESTRICTION_TABLES: ReadonlySet<string> = new Set([
  "rules_profile",
  "restriction",
  "rules_name",
  "face_rules_name",
])

/** Files that carry the selected edition's restriction tables; empty when the snapshot has no coverage. */
export function restrictionKeys(manifest: JsonObject, files: Files, edition: Region): string[] {
  if (!arrayValue(manifest["restriction_coverage"] ?? []).length) return []
  return [...files]
    .filter(
      ([, file]) =>
        (file["region"] === null || file["region"] === edition) &&
        arrayValue(file["row_counts"]).some((count) =>
          RESTRICTION_TABLES.has(stringValue(arrayValue(count)[0])),
        ),
    )
    .map(([key]) => key)
}

const contains = (row: Row, day: string, from: string, until: string) =>
  typeof row[from] === "string" &&
  row[from] <= day &&
  (row[until] === null || (typeof row[until] === "string" && day < row[until]))

/** A source cutoff cannot exceed the snapshot date; missing coverage never proves absence. */
export function selectRestrictions(
  manifest: JsonObject,
  rows: (table: string) => readonly Row[],
  index: CardIndex,
  edition: Region,
  requestedDate?: string,
): RestrictionSelection {
  let onDate = requestedDate ?? stringValue(manifest["data_as_of"])
  const cards = new Set<string>()
  const details = new Map<string, CardRestriction[]>()
  const unknown = () => ({ status: { state: "unknown", onDate }, cards, details }) as const
  if (onDate > stringValue(manifest["data_as_of"])) return unknown()
  const profiles = rows("rules_profile").filter(
    (row) => row["region"] === edition && row["format_code"] === "standard",
  )
  if (profiles.length !== 1) return unknown()
  const profile = profiles[0]
  const sourceWindows = arrayValue(manifest["restriction_coverage"] ?? [])
    .map((value) => objectValue(value))
    .filter((row) => row["profile_id"] === profile?.["id"])
  const snapshotDate = stringValue(manifest["data_as_of"])
  const coveredDates = sourceWindows.flatMap((window) => {
    const from = stringValue(window["from_date"])
    if (from > snapshotDate) return []
    const until = window["until_date"]
    // Coverage has an exclusive end; a one-day source window pins its exact list date.
    const last =
      typeof until === "string"
        ? new Date(Date.parse(`${until}T00:00:00Z`) - 86_400_000).toISOString().slice(0, 10)
        : snapshotDate
    const day = last < snapshotDate ? last : snapshotDate
    return day < from ? [] : [day]
  })
  if (requestedDate === undefined) onDate = coveredDates.sort().at(-1) ?? snapshotDate
  const revisions = arrayValue(profile?.["revisions"] ?? [])
    .map((value) => objectValue(value))
    .filter((row) => contains(row, onDate, "effective_from", "effective_until"))
  if (revisions.length > 1) return unknown()
  const windows = arrayValue(manifest["restriction_coverage"])
    .map((value) => objectValue(value))
    .filter(
      (row) =>
        row["profile_id"] === profile?.["id"] && contains(row, onDate, "from_date", "until_date"),
    )
  if (windows.length !== 1) return unknown()
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
        (limit > 1 && (typeof baseline !== "number" || limit >= baseline)))
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
        if (!region || (scope !== "all" && region["deck_role"] == null)) {
          incomplete = true
          continue
        }
        if (scope === "all" || region["deck_role"] === scope) {
          const cardId = stringValue(face["card_id"])
          cards.add(cardId)
          const entries = details.get(cardId) ?? []
          if (!entries.some((entry) => entry.id === restriction["id"]))
            entries.push({
              id: stringValue(restriction["id"]),
              formatCode: "standard",
              maxCopies: typeof limit === "number" ? limit : null,
              effectiveFrom: stringValue(restriction["effective_from"]),
              sourceUrl:
                typeof windows[0]?.["source_url"] === "string" ? windows[0]["source_url"] : null,
              asOf: onDate,
            })
          details.set(cardId, entries)
        }
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
    details,
  }
}

export async function loadCardRestrictionDetails(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  edition: Region,
): Promise<CardRestrictionDetails> {
  const keys = restrictionKeys(snapshot.manifest, snapshot.files, edition)
  if (!keys.length)
    return {
      status: { state: "unknown", onDate: stringValue(snapshot.manifest["data_as_of"]) },
      restrictions: [],
    }
  const fragments = [
    ...snapshot.bootstrap,
    ...(await Promise.all(keys.map((key) => client.fragments(key)))).flat(),
  ]
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  const selection = selectRestrictions(
    snapshot.manifest,
    (table) => mergedRows(fragments, table),
    createCardIndex(snapshot),
    edition,
  )
  return { status: selection.status, restrictions: selection.details.get(cardId) ?? [] }
}
