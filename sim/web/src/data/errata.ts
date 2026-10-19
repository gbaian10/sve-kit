import type { LoadedSnapshot, SnapshotClient } from "./client"
import { arrayValue, type JsonObject, objectValue, stringValue } from "./format-v3/json"

export interface CardErratum {
  readonly url: string
  readonly version: JsonObject
  readonly changes: readonly JsonObject[]
}

export async function loadCardErrata(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  printingId: string,
  faceId: string,
  region: string,
): Promise<CardErratum[]> {
  if (!arrayValue(objectValue(snapshot.manifest["errata_card_ids"])[region]).includes(cardId))
    return []
  const keys = [...snapshot.files]
    .filter(([, file]) =>
      arrayValue(file["row_counts"]).some((count) => arrayValue(count)[0] === "errata"),
    )
    .map(([key]) => key)
  const fragments = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  return fragments
    .filter((fragment) => fragment.table === "errata")
    .flatMap((fragment) => fragment.rows)
    .filter((notice) => notice["region"] === region)
    .flatMap((notice) =>
      arrayValue(notice["versions"])
        .map((value) => objectValue(value))
        .filter((version) =>
          arrayValue(version["printings"]).some(
            (printing) => objectValue(printing)["printing_id"] === printingId,
          ),
        )
        .map((version) => ({
          url: stringValue(notice["official_url"]),
          version,
          changes: arrayValue(version["changes"])
            .map((value) => objectValue(value))
            .filter((change) => change["face_id"] === faceId),
        })),
    )
    .sort(
      (a, b) =>
        stringValue(a.version["announced_on"] ?? "").localeCompare(
          stringValue(b.version["announced_on"] ?? ""),
        ) || a.url.localeCompare(b.url),
    )
}
