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

/** Resolves the URL to a printing once the snapshot and this key's route rows are available. */
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
    const routeKey =
      params.intId === undefined
        ? { namespace: "official" as const, value: params.cardNo ?? "" }
        : { namespace: "provisional" as const, value: params.intId }
    createRouteLookups(catalog, globalDetailOf(client, catalog.index), routeKey).then(
      (lookups) => {
        if (cancelled) return
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
