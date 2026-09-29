import { useEffect, useState } from "react"

import {
  type Catalog,
  type EffectPreview,
  loadEffectPreview,
  type SnapshotClient,
  type TextContext,
  textContextOf,
} from "../data"

/**
 * Effect texts for the table view's visible rows, fetched as rows appear; a row without text (or
 * one that failed) simply shows no preview. Results are kept per snapshot client.
 */
export function useEffectPreviews(
  client: SnapshotClient,
  catalog: Catalog | null,
  printingIds: readonly string[],
  enabled: boolean,
): (printingId: string) => EffectPreview | undefined {
  const [previews, setPreviews] = useState<{
    readonly catalog: Catalog | null
    readonly texts: ReadonlyMap<string, EffectPreview | null>
  }>({ catalog: null, texts: new Map() })
  const texts =
    previews.catalog === catalog ? previews.texts : new Map<string, EffectPreview | null>()
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
        for (const [id, preview] of loaded) next.set(id, preview ?? null)
        return { catalog, texts: next }
      })
    })
    return () => {
      cancelled = true
    }
  }, [client, catalog, missing])
  return (printingId) => texts.get(printingId) ?? undefined
}

/** The icons and keyword names the table rows render their previews with, once loaded. */
export function useTextContext(
  client: SnapshotClient,
  catalog: Catalog | null,
  enabled: boolean,
): TextContext | undefined {
  const [loaded, setLoaded] = useState<{
    readonly catalog: Catalog
    readonly context: TextContext
  } | null>(null)
  const context = loaded !== null && loaded.catalog === catalog ? loaded.context : undefined
  useEffect(() => {
    if (!enabled || !catalog || context !== undefined) return
    let cancelled = false
    void textContextOf(client, catalog).then(
      (built) => {
        if (!cancelled) setLoaded({ catalog, context: built })
      },
      () => undefined,
    )
    return () => {
      cancelled = true
    }
  }, [client, catalog, enabled, context])
  return context
}
