import {
  arrayValue,
  type Catalog,
  catalogOf,
  type LoadedSnapshot,
  type SnapshotClient,
  stringValue,
} from "../data"

// The image index and the catalog only resolve printings whose rows are in a snapshot; the route
// completes just the opened printing, so the bands of these cards' other printings are pinned here.
export async function pinPrintings(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  locators: Catalog,
  cardIds: readonly string[],
  regions: readonly ("jp" | "en")[],
): Promise<SnapshotClient> {
  const owners = new Set(
    cardIds.flatMap((cardId) =>
      locators.index
        .printingsOf(cardId)
        .filter((row) => regions.includes(row["region"] as "jp" | "en"))
        .map((row) => row["home_set_id"]),
    ),
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

/** The pinned client and the catalog that lists every printing of the given cards with its summary. */
export async function relatedPrintings(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  locators: Catalog,
  cardIds: readonly string[],
  regions: readonly ("jp" | "en")[],
): Promise<{ readonly client: SnapshotClient; readonly catalog: Catalog }> {
  const pinned = await pinPrintings(client, snapshot, locators, cardIds, regions)
  const supplemented = pinned.snapshot()
  if (!supplemented) throw new Error("snapshot replaced")
  return { client: pinned, catalog: catalogOf(supplemented) }
}
