import { sharedAnnotatedTextResolver } from "./annotated-text"
import { catalogOf } from "./catalog"
import type { LoadedSnapshot, SnapshotClient } from "./client"
import { cardPrintings } from "./edition"
import { type PrintedWording, reconstructPrintedWording } from "./errata-reconstruction"
import { loadFaceCandidates } from "./face-basics"
import { arrayValue, type JsonObject, objectValue, stringValue } from "./format-v3/json"

export interface CardErratum {
  readonly url: string
  readonly version: JsonObject
  readonly changes: readonly JsonObject[]
  readonly printed: PrintedWording
  readonly printings: readonly { readonly cardNo: string; readonly confirmed: boolean }[]
}

const cached = new WeakMap<
  SnapshotClient,
  { snapshot: LoadedSnapshot; entries: Map<string, Promise<CardErratum[]>> }
>()

export function loadCardErrata(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  printingId: string,
  faceId: string,
  region: string,
): Promise<CardErratum[]> {
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    return Promise.reject(new Error("snapshot replaced"))
  let cache = cached.get(client)
  if (cache?.snapshot !== snapshot) {
    cache = { snapshot, entries: new Map() }
    cached.set(client, cache)
  }
  const key = JSON.stringify([cardId, printingId, faceId, region])
  const present = cache.entries.get(key)
  if (present) return present
  const entries = cache.entries
  const result = readCardErrata(client, snapshot, cardId, printingId, faceId, region).catch(
    (error: unknown) => {
      entries.delete(key)
      throw error
    },
  )
  entries.set(key, result)
  return result
}

async function readCardErrata(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  printingId: string,
  faceId: string,
  region: string,
): Promise<CardErratum[]> {
  if (!arrayValue(objectValue(snapshot.manifest["errata_card_ids"])[region] ?? []).includes(cardId))
    return []
  const keys = [...snapshot.files]
    .filter(([, file]) =>
      arrayValue(file["row_counts"]).some((count) => arrayValue(count)[0] === "errata"),
    )
    .map(([key]) => key)
  const fragments = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  const notices = fragments
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
  if (!notices.length) return []
  const [candidateSnapshot, printingCatalog] = await Promise.all([
    loadFaceCandidates(client, snapshot, cardId, region),
    cardPrintings(client, snapshot, cardId, [region === "jp" ? "jp" : "en"]),
  ])
  const index = catalogOf(candidateSnapshot).index
  const revision =
    index.currentRevision(faceId, region) ?? index.displayRevision(faceId, region, printingId)
  const revisionId = revision?.["id"]
  const effectNotices = notices.filter((notice) =>
    notice.changes.some((change) => change["field"] === "effect"),
  )
  const selected =
    typeof revisionId === "string" && effectNotices.length === 1
      ? await sharedAnnotatedTextResolver(client, cardId).resolve(
          { owner: { kind: "face_revision", id: revisionId }, field: "effect", ordinal: null },
          region === "jp" ? "ja" : "en",
        )
      : null
  if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
    throw new Error("snapshot replaced")
  return notices.map((notice): CardErratum => {
    const printings = arrayValue(notice.version["printings"]).map((value) => objectValue(value))
    const confirmed = printings.some(
      (printing) =>
        printing["printing_id"] === printingId && printing["scope"] === "confirmed_applies",
    )
    return {
      ...notice,
      printings: printings.map((printing) => ({
        cardNo: stringValue(
          printingCatalog.index.printing(stringValue(printing["printing_id"]))?.["card_no"],
        ),
        confirmed: printing["scope"] === "confirmed_applies",
      })),
      printed:
        effectNotices.length > 1
          ? { state: "unknown", reason: "multiple_notices" }
          : reconstructPrintedWording(
              selected ? stringValue(selected.original.unit["text"]) : undefined,
              notice.changes.filter((change) => change["field"] === "effect"),
              confirmed,
            ),
    }
  })
}
