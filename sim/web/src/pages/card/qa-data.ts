import {
  arrayValue,
  createLocator,
  GLOBAL_OWNER,
  type JsonObject,
  type LoadedSnapshot,
  objectValue,
  type SnapshotClient,
  stringValue,
} from "../../data"

export interface QaEntry {
  readonly qaId: string
  readonly versionId: string
  /** The official number as printed on the source page, e.g. "Q643". */
  readonly number: string | null
  readonly url: string | null
}

/**
 * What the snapshot knows about one card's official Q&A in one edition. An empty list is never
 * "absent": only a complete source window may say the official Q&A has none.
 */
export type CardQa =
  | { readonly coverage: "present"; readonly entries: readonly QaEntry[] }
  | { readonly coverage: "absent" }
  | { readonly coverage: "unknown" }

const numberOf = (entry: QaEntry) =>
  Number(/\d+/u.exec(entry.number ?? "")?.[0] ?? Number.MAX_SAFE_INTEGER)

const tableFiles = (snapshot: LoadedSnapshot, table: string, region?: string) =>
  [...snapshot.files]
    .filter(
      ([, file]) =>
        arrayValue(file["row_counts"]).some((count) => arrayValue(count)[0] === table) &&
        (region === undefined || file["region"] === region),
    )
    .map(([key]) => key)

/** A complete source window of the whole edition that covers `queryDate` proves the absence. */
function windowCovers(snapshot: LoadedSnapshot, region: string, queryDate: string): boolean {
  const windows = arrayValue(snapshot.manifest["source_windows"])
    .map((raw) => objectValue(raw))
    .filter((window) => window["kind"] === "qa" && window["region"] === region)
  return windows.some((window) => {
    const until = window["until_date"]
    return (
      window["scope_key"] === "region:*" &&
      window["state"] === "complete" &&
      stringValue(window["from_date"]) <= queryDate &&
      (typeof until !== "string" || queryDate < until) &&
      queryDate <= stringValue(window["as_of"]) &&
      !windows.some((partial) => {
        const partialUntil = partial["until_date"]
        return (
          partial["state"] === "partial" &&
          stringValue(partial["from_date"]) <= stringValue(window["as_of"]) &&
          (typeof until !== "string" || stringValue(partial["from_date"]) < until) &&
          stringValue(window["from_date"]) <= stringValue(partial["as_of"]) &&
          (typeof partialUntil !== "string" || stringValue(window["from_date"]) < partialUntil)
        )
      })
    )
  })
}

const readRows = async (client: SnapshotClient, keys: readonly string[], table: string) =>
  (await Promise.all([...new Set(keys)].map((key) => client.fragments(key))))
    .flat()
    .filter((fragment) => fragment.table === table)
    .flatMap((fragment) => fragment.rows)

async function indexedEntries(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  region: string,
  ids: readonly string[],
): Promise<QaEntry[]> {
  const locate = createLocator(snapshot.files, new Set(["qa", "qa_version"]))
  const keyOf = (table: string, id: string, edition: string | null) => {
    const key = locate({
      table,
      owner: GLOBAL_OWNER,
      entityKey: [id],
      partition: "detail",
      region: edition,
      purpose: "detail",
    })
    if (!key) throw new Error("Q&A index target is unavailable")
    return key
  }
  const versions = new Map(
    (
      await readRows(
        client,
        ids.map((id) => keyOf("qa_version", id, null)),
        "qa_version",
      )
    ).map((row) => [stringValue(row["id"]), row]),
  )
  const selected = ids.map((id) => {
    const version = versions.get(id)
    if (!version || !arrayValue(version["cards"]).includes(cardId))
      throw new Error("Q&A index version differs from its card")
    return version
  })
  const questions = new Map(
    (
      await readRows(
        client,
        selected.map((version) => keyOf("qa", stringValue(version["qa_id"]), region)),
        "qa",
      )
    ).map((row) => [stringValue(row["id"]), row]),
  )
  return selected.map((version) => {
    const qa = questions.get(stringValue(version["qa_id"]))
    if (qa?.["region"] !== region || qa["current_version_id"] !== version["id"])
      throw new Error("Q&A index does not name this edition's current version")
    return entryOf(qa, version)
  })
}

const entryOf = (qa: JsonObject, version: JsonObject): QaEntry => ({
  qaId: stringValue(qa["id"]),
  versionId: stringValue(version["id"]),
  number: typeof qa["official_number"] === "string" ? qa["official_number"] : null,
  url: typeof qa["source_url"] === "string" ? qa["source_url"] : null,
})

/**
 * A complete indexed edition needs only the selected rows. Incomplete editions keep the
 * full scan, where the union summary only gates reads and the current rows decide membership.
 */
export async function loadCardQa(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  region: string,
  queryDate: string,
): Promise<CardQa> {
  const complete = windowCovers(snapshot, region, queryDate)
  const index = snapshot.manifest["qa_card_index"]
  if (index !== undefined && complete) {
    const edition = objectValue(objectValue(index)[region])
    const ids = arrayValue(edition[cardId] ?? []).map((id) => stringValue(id))
    const entries = ids.length ? await indexedEntries(client, snapshot, cardId, region, ids) : []
    if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
      throw new Error("snapshot replaced")
    return entries.length ? { coverage: "present", entries } : { coverage: "absent" }
  }
  const entries: QaEntry[] = []
  if (arrayValue(snapshot.manifest["qa_card_ids"] ?? []).includes(cardId)) {
    const [questions, versions] = await Promise.all([
      readRows(client, tableFiles(snapshot, "qa", region), "qa"),
      readRows(client, tableFiles(snapshot, "qa_version"), "qa_version"),
    ])
    if (client.snapshot()?.manifestHash !== snapshot.manifestHash)
      throw new Error("snapshot replaced")
    const mine = new Map(
      versions
        .filter((version) => arrayValue(version["cards"]).includes(cardId))
        .map((version) => [stringValue(version["id"]), version]),
    )
    for (const qa of questions) {
      const versionId = qa["current_version_id"]
      const version = typeof versionId === "string" ? mine.get(versionId) : undefined
      if (qa["region"] !== region || !version) continue
      entries.push(entryOf(qa, version))
    }
  }
  if (entries.length > 0)
    return {
      coverage: "present",
      entries: entries.sort(
        (a, b) => numberOf(a) - numberOf(b) || (a.number ?? "").localeCompare(b.number ?? ""),
      ),
    }
  return { coverage: complete ? "absent" : "unknown" }
}
