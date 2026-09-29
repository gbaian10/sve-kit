import { useEffect, useState } from "react"

import { type CardView, type Catalog, loadCardView, type SnapshotClient } from "../data"
import type { UiLanguage } from "../i18n/languages"

export type CardViewState =
  | { readonly status: "loading" }
  | { readonly status: "failed" }
  | { readonly status: "missing" }
  | { readonly status: "ready"; readonly view: CardView }

/** The card page's data for one printing; reloads when the printing or the UI language changes. */
export function useCardView(
  client: SnapshotClient,
  catalog: Catalog | null,
  printingId: string | undefined,
  uiLanguage: UiLanguage,
): CardViewState {
  const [state, setState] = useState<{ readonly key: string; readonly value: CardViewState }>()
  const key = `${catalog === null ? "-" : "c"}\u0000${printingId ?? ""}\u0000${uiLanguage}`
  useEffect(() => {
    if (!catalog || printingId === undefined) return
    let cancelled = false
    loadCardView(client, catalog, printingId, uiLanguage).then(
      (view) => {
        if (cancelled) return
        setState({ key, value: view ? { status: "ready", view } : { status: "missing" } })
      },
      () => {
        if (!cancelled) setState({ key, value: { status: "failed" } })
      },
    )
    return () => {
      cancelled = true
    }
  }, [client, catalog, printingId, uiLanguage, key])
  return state !== undefined && state.key === key ? state.value : { status: "loading" }
}
