import type { LoadedSnapshot, SnapshotClient } from "./client"
import { arrayValue, type JsonObject, objectValue, stringValue } from "./format-v3/json"
import { mergedRows } from "./format-v3/reader"

/** One credited artist, named exactly as the official site prints the credit. */
export interface FaceArtist {
  readonly id: string
  readonly name: string
}

const hasTable = (file: JsonObject, table: string, owner?: string): boolean =>
  arrayValue(file["row_counts"]).some((raw) => {
    const row = arrayValue(raw)
    return row[0] === table && (owner === undefined || row[2] === owner)
  })

/**
 * The artists credited on the given arts in one edition. Art rows sit in the print files of the
 * card's home set and the names in the few shared artist files, so only those are read.
 */
export async function artCredits(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  owner: string,
  edition: "jp" | "en",
  artIds: readonly string[],
): Promise<ReadonlyMap<string, readonly FaceArtist[]>> {
  const keys = [...snapshot.files]
    .filter(
      ([, file]) =>
        file["purpose"] === "print" && (hasTable(file, "art", owner) || hasTable(file, "artist")),
    )
    .map(([key]) => key)
  const parts = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  const names = new Map(
    mergedRows(parts, "artist").map((row) => [
      stringValue(row["id"]),
      stringValue(row["display_name"]),
    ]),
  )
  const wanted = new Set(artIds)
  const credits = new Map<string, FaceArtist[]>()
  for (const row of mergedRows(parts, "art")) {
    const art = stringValue(row["id"])
    if (!wanted.has(art)) continue
    const artists = arrayValue(row["artists"] ?? [])
      .map((raw) => objectValue(raw))
      .filter((credit) => credit["role"] === `illustration_${edition}`)
      .flatMap((credit) => {
        const id = stringValue(credit["artist_id"])
        const name = names.get(id)
        return name === undefined ? [] : [{ id, name }]
      })
    if (artists.length) credits.set(art, artists)
  }
  return credits
}
