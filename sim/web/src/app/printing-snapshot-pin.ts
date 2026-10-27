import {
  arrayValue,
  type Catalog,
  type LoadedSnapshot,
  type SnapshotClient,
  stringValue,
} from "../data"

// The image index and the catalog only resolve printings whose rows are in a snapshot; the route
// completes just the opened printing, so the bands of these cards' other printings are pinned here.
// The base may be the other region's catalog: the client's own bootstrap is merged in so both
// regions' rows are present, as `cardPrintings` does.
export async function pinPrintings(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  locators: Catalog,
  cardIds: readonly string[],
  regions: readonly ("jp" | "en")[],
): Promise<SnapshotClient> {
  const current = client.snapshot()
  if (current && current !== snapshot && current.manifestHash === snapshot.manifestHash) {
    const present = new Set(snapshot.bootstrap.map((fragment) => fragment.identity))
    snapshot = {
      ...snapshot,
      bootstrap: [
        ...snapshot.bootstrap,
        ...current.bootstrap.filter((fragment) => !present.has(fragment.identity)),
      ],
    }
  }
  // A region's catalog locates only its own printings, so the other region's locators come from
  // the merged bootstrap rows (printing rows and card locators) rather than the page's catalog.
  const wanted = new Set(cardIds)
  const owners = new Set([
    ...cardIds.flatMap((cardId) =>
      locators.index
        .printingsOf(cardId)
        .filter((row) => regions.includes(row["region"] as "jp" | "en"))
        .map((row) => row["home_set_id"]),
    ),
    ...snapshot.bootstrap
      .filter((fragment) => fragment.table === "printing" || fragment.table === "card_locator")
      .flatMap((fragment) => fragment.rows)
      .filter(
        (row) =>
          typeof row["card_id"] === "string" &&
          wanted.has(row["card_id"]) &&
          regions.includes(row["region"] as "jp" | "en"),
      )
      .map((row) => row["home_set_id"]),
  ])
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
