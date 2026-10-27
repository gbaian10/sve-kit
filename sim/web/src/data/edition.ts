import { type CardRouteInput, locateCard } from "./card-route"
import { type CardSummary, type Catalog, catalogOf } from "./catalog"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import { type Row } from "./format-v3/decode"
import { arrayValue, objectValue, stringValue } from "./format-v3/json"

/** Complete the selected card's printing locators without loading unrelated printings. */
export async function cardPrintings(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  regions: readonly ("jp" | "en")[] = [snapshot.region ?? "jp"],
): Promise<Catalog> {
  const current = client.snapshot()
  if (current?.manifestHash === snapshot.manifestHash) {
    const present = new Set(snapshot.bootstrap.map((fragment) => fragment.identity))
    const additional = current.bootstrap.filter((fragment) => !present.has(fragment.identity))
    if (additional.length)
      snapshot = { ...snapshot, bootstrap: [...snapshot.bootstrap, ...additional] }
  }
  const catalog = catalogOf(snapshot)
  const owners = new Set(
    catalog.index
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
          const row = arrayValue(raw)
          return row[0] === "printing" && owners.has(row[2])
        }),
    )
    .map(([key]) => key)
  const parts = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  const present = new Set(snapshot.bootstrap.map((fragment) => fragment.identity))
  const additional = parts.filter((part) => !present.has(part.identity))
  return additional.length
    ? catalogOf({ ...snapshot, bootstrap: [...snapshot.bootstrap, ...additional] })
    : catalog
}

/** Art identities are authored across regions; card numbers do not establish that relationship. */
function editionPrinting(source: Row, target: Catalog, region: "jp" | "en"): Row | undefined {
  const cardId = stringValue(source["card_id"])
  const candidates = target.index.printingsOf(cardId).filter((row) => row["region"] === region)
  const card = target.index.card(cardId)
  const defaultId = arrayValue(card?.["regions"] ?? [])
    .map((raw) => objectValue(raw))
    .find((view) => view["region"] === region)?.["default_printing_id"]
  const arts = new Set<unknown>(
    arrayValue(source["faces"] ?? [])
      .map((raw) => objectValue(raw)["art_id"])
      .filter((id) => typeof id === "string"),
  )
  const matches = candidates.filter((row) =>
    arrayValue(row["faces"] ?? []).some((raw) => arts.has(objectValue(raw)["art_id"])),
  )
  const preferred = matches.length ? matches : candidates
  return preferred.find((row) => row["id"] === defaultId) ?? preferred[0]
}

export async function locateEdition(
  client: SnapshotClient,
  input: CardRouteInput,
  region: "jp" | "en",
  signal: AbortSignal,
): Promise<CardSummary | undefined> {
  const source = await locateCard(client, input)
  signal.throwIfAborted()
  if (!client.catalog) throw new Error("catalog unavailable")
  const snapshot = await client.catalog(region, signal)
  const catalog = await cardPrintings(client, snapshot, source.printing.cardId)
  signal.throwIfAborted()
  const current = source.catalog.index.printing(source.printing.id)
  if (!current) throw new Error("source printing unavailable")
  const target = editionPrinting(current, catalog, region)
  if (!target) return undefined
  const summary = catalog.summary(stringValue(target["id"]))
  if (!summary) throw new Error("target printing unavailable")
  return summary
}
