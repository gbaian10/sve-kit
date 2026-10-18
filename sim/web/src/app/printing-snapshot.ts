import {
  arrayValue,
  type Catalog,
  type LoadedSnapshot,
  type SnapshotClient,
  stringValue,
} from "../data"

/**
 * A client whose snapshot carries the printing rows of every listed printing of one card, so the
 * image index can resolve thumbnails of printings that live outside the card's own family. The
 * route only completes the printing it opened; the printing catalog holds the others without
 * their rows being part of any snapshot.
 */
export async function printingImageClient(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  printings: Catalog,
  cardId: string,
  regions: readonly ("jp" | "en")[],
): Promise<SnapshotClient> {
  const owners = new Set(
    printings.index
      .printingsOf(cardId)
      .filter((row) => regions.includes(row["region"] as "jp" | "en"))
      .map((row) => row["home_set_id"]),
  )
  const keys = [...snapshot.files]
    .filter(
      ([, file]) =>
        file["purpose"] === "print" &&
        regions.includes(file["region"] as "jp" | "en") &&
        arrayValue(file["row_counts"]).some((raw) => {
          const count = arrayValue(raw)
          return count[0] === "printing" && owners.has(count[2])
        }),
    )
    .map(([key]) => key)
  const parts = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  const present = new Set(snapshot.bootstrap.map((part) => part.identity))
  const supplemented: LoadedSnapshot = {
    ...snapshot,
    bootstrap: [...snapshot.bootstrap, ...parts.filter((part) => !present.has(part.identity))],
  }
  return { ...client, snapshot: () => supplemented, base: stringValue(client.base) }
}
