import { arrayValue, type LoadedSnapshot, type Row, type SnapshotClient } from "../data"

export interface CardRelations {
  readonly related: readonly Row[]
  readonly mentionedBy: readonly Row[]
}

// Incoming references live with the target, so cross-family sources need no extra engine reads.
// Keep all official rows in these bands for renamed evolve-pair detection.
export async function loadCardRelated(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  homeSetId: string,
): Promise<CardRelations> {
  const keys = [...snapshot.files]
    .filter(
      ([, file]) =>
        file["purpose"] === "engine" &&
        arrayValue(file["row_counts"]).some((raw) => {
          const count = arrayValue(raw)
          return (
            (count[0] === "card_related" || count[0] === "card_mentioned_by") &&
            count[1] === "home_set" &&
            count[2] === homeSetId
          )
        }),
    )
    .map(([key]) => key)
  const parts = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  return {
    related: parts.filter((fragment) => fragment.table === "card_related").flatMap((f) => f.rows),
    mentionedBy: parts
      .filter((fragment) => fragment.table === "card_mentioned_by")
      .flatMap((f) => f.rows),
  }
}
