import { type Catalog, catalogOf, type LoadedSnapshot, type SnapshotClient } from "../data"
import { pinPrintings } from "./printing-snapshot-pin"

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
