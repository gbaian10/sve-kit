import { useEffect, useState } from "react"

import { type Catalog, createRouteLookups, globalDetailOf, type SnapshotClient } from "../data"
import { resolveCardRoute, resolveProvisionalRoute, type RouteResolution } from "../domain/route"

export interface CardRouteParams {
  readonly cardNo?: string
  readonly slug?: string
  readonly intId?: string
}

export type CardRouteState =
  | { readonly status: "loading" }
  | { readonly status: "failed" }
  | ({ readonly status: "resolved" } & RouteResolution)

/** Resolves the URL to a printing once the snapshot and its route tables are available. */
export function useCardRoute(
  client: SnapshotClient,
  catalog: Catalog | null,
  params: CardRouteParams,
): CardRouteState {
  const [state, setState] = useState<{ readonly key: string; readonly value: CardRouteState }>()
  const key = `${catalog === null ? "-" : "c"}\u0000${params.cardNo ?? ""}\u0000${params.slug ?? ""}\u0000${params.intId ?? ""}`
  useEffect(() => {
    if (!catalog) return
    let cancelled = false
    globalDetailOf(client, catalog.index).then(
      (global) => {
        if (cancelled) return
        const lookups = createRouteLookups(catalog, global)
        const resolution =
          params.intId !== undefined
            ? resolveProvisionalRoute(params.intId, lookups)
            : resolveCardRoute(params.cardNo ?? "", params.slug, lookups)
        setState({ key, value: { status: "resolved", ...resolution } })
      },
      () => {
        if (!cancelled) setState({ key, value: { status: "failed" } })
      },
    )
    return () => {
      cancelled = true
    }
  }, [client, catalog, params.cardNo, params.slug, params.intId, key])
  return state !== undefined && state.key === key ? state.value : { status: "loading" }
}
