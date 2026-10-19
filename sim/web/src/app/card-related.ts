import { arrayValue, type LoadedSnapshot, type Row, type SnapshotClient } from "../data"

// Only the card's own family bands are read: the official lists already carry both directions.
// Every row of those bands is returned, not just the card's own: a renamed evolve card is paired
// from the rows of the other side, which usually sits in the same family.
export async function loadCardRelated(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  homeSetId: string,
): Promise<readonly Row[]> {
  const keys = [...snapshot.files]
    .filter(
      ([, file]) =>
        file["purpose"] === "engine" &&
        arrayValue(file["row_counts"]).some((raw) => {
          const count = arrayValue(raw)
          return count[0] === "card_related" && count[1] === "home_set" && count[2] === homeSetId
        }),
    )
    .map(([key]) => key)
  const parts = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  return parts
    .filter((fragment) => fragment.table === "card_related")
    .flatMap((fragment) => fragment.rows)
}
