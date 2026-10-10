import type { SnapshotClient } from "./client"
import { integerValue, objectValue, stringValue } from "./format-v3/json"
import { bucketOf, createLocator, GLOBAL_OWNER } from "./locator"
import type { CardIndex } from "./store"

export interface TextResolver {
  /** The text if it is already in memory (bootstrap or a loaded detail file), else undefined. */
  readonly textOf: (unitId: string) => string | undefined
  /** Loads whichever detail files hold these units, then `textOf` answers for them. */
  readonly ensure: (unitIds: readonly string[]) => Promise<void>
}

export function createTextResolver(client: SnapshotClient, index: CardIndex): TextResolver {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const locate = createLocator(snapshot.files)
  const bucketCount = integerValue(objectValue(snapshot.manifest["partitioning"])["bucket_count"])
  const loaded = new Map<string, string>()
  const textOf = (unitId: string): string | undefined => {
    const row = index.textUnit(unitId)
    if (row) return stringValue(row["text"])
    return loaded.get(unitId)
  }
  const ensure = async (unitIds: readonly string[]): Promise<void> => {
    const keys = new Set<string>()
    for (const unitId of unitIds) {
      if (textOf(unitId) !== undefined) continue
      const key = locate({
        table: "text_unit",
        owner: GLOBAL_OWNER,
        bucket: bucketOf([unitId], bucketCount),
        partition: "detail",
      })
      if (key !== undefined) keys.add(key)
    }
    await Promise.all(
      [...keys].map(async (key) => {
        for (const fragment of await client.fragments(key)) {
          if (fragment.table !== "text_unit") continue
          for (const row of fragment.rows)
            loaded.set(stringValue(row["id"]), stringValue(row["text"]))
        }
      }),
    )
  }
  return { textOf, ensure }
}
