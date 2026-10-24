import {
  arrayValue,
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

/** A complete source window of the whole edition that covers `today` proves the absence. */
function windowCovers(snapshot: LoadedSnapshot, region: string, today: string): boolean {
  const windows = arrayValue(snapshot.manifest["source_windows"]).map((raw) => objectValue(raw))
  const applicable = windows.filter((window) => {
    const until = window["until_date"]
    return (
      window["kind"] === "qa" &&
      window["region"] === region &&
      stringValue(window["from_date"]) <= today &&
      (typeof until !== "string" || today < until) &&
      today <= stringValue(window["as_of"])
    )
  })
  // A partial window can contradict the edition-wide claim even when its scope is narrower.
  return (
    applicable.some(
      (window) => window["scope_key"] === "region:*" && window["state"] === "complete",
    ) && applicable.every((window) => window["state"] === "complete")
  )
}

/**
 * Reads the current version of every Q&A of this edition that names the card. The summary list
 * `qa_card_ids` only gates the file reads; the rows decide what is shown.
 */
export async function loadCardQa(
  client: SnapshotClient,
  snapshot: LoadedSnapshot,
  cardId: string,
  region: string,
  today: string,
): Promise<CardQa> {
  const entries: QaEntry[] = []
  if (arrayValue(snapshot.manifest["qa_card_ids"] ?? []).includes(cardId)) {
    const read = async (keys: readonly string[], table: string): Promise<JsonObject[]> =>
      (await Promise.all(keys.map((key) => client.fragments(key))))
        .flat()
        .filter((fragment) => fragment.table === table)
        .flatMap((fragment) => fragment.rows)
    const [questions, versions] = await Promise.all([
      read(tableFiles(snapshot, "qa", region), "qa"),
      read(tableFiles(snapshot, "qa_version"), "qa_version"),
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
      entries.push({
        qaId: stringValue(qa["id"]),
        versionId: stringValue(version["id"]),
        number: typeof qa["official_number"] === "string" ? qa["official_number"] : null,
        url: typeof qa["source_url"] === "string" ? qa["source_url"] : null,
      })
    }
  }
  if (entries.length > 0)
    return {
      coverage: "present",
      entries: entries.sort(
        (a, b) => numberOf(a) - numberOf(b) || (a.number ?? "").localeCompare(b.number ?? ""),
      ),
    }
  return { coverage: windowCovers(snapshot, region, today) ? "absent" : "unknown" }
}
