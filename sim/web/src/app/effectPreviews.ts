import { useEffect, useState } from "react"

import { type Catalog, loadEffectPreview, type SnapshotClient } from "../data"

/**
 * Effect texts for the table view's visible rows, fetched as rows appear; a row without text (or
 * one that failed) simply shows no preview. Results are kept per snapshot client.
 */
export function useEffectPreviews(
  client: SnapshotClient,
  catalog: Catalog | null,
  printingIds: readonly string[],
  enabled: boolean,
): (printingId: string) => string | undefined {
  const [previews, setPreviews] = useState<{
    readonly catalog: Catalog | null
    readonly texts: ReadonlyMap<string, string>
  }>({ catalog: null, texts: new Map() })
  const texts = previews.catalog === catalog ? previews.texts : new Map<string, string>()
  const missing =
    enabled && catalog ? printingIds.filter((id) => !texts.has(id)).join("\u0000") : ""
  useEffect(() => {
    if (!catalog || missing === "") return
    let cancelled = false
    const ids = missing.split("\u0000")
    void Promise.all(
      ids.map(
        async (id) =>
          [id, await loadEffectPreview(client, catalog, id).catch(() => undefined)] as const,
      ),
    ).then((loaded) => {
      if (cancelled) return
      setPreviews((current) => {
        const next = new Map(current.catalog === catalog ? current.texts : [])
        for (const [id, text] of loaded) next.set(id, text ?? "")
        return { catalog, texts: next }
      })
    })
    return () => {
      cancelled = true
    }
  }, [client, catalog, missing])
  return (printingId) => {
    const text = texts.get(printingId)
    return text === undefined || text === "" ? undefined : text
  }
}
