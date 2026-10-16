import { useEffect, useState } from "react"

import { catalogOf, type LoadedSnapshot, type SnapshotClient } from "../data"
import type { Region } from "../domain/search"

/** The active catalog stays usable while the comparison downloads independently. */
export function useOtherEdition(
  client: SnapshotClient,
  edition: Region,
  enabled: boolean,
  ready: boolean,
  manifestHash?: string,
) {
  const [attempt, setAttempt] = useState(0)
  const [result, setResult] = useState<{
    key: string
    snapshot?: LoadedSnapshot
    error?: unknown
  }>()
  const key = `${client.base}/${manifestHash ?? ""}/${edition}/${String(attempt)}`
  useEffect(() => {
    if (!enabled || !ready || !client.catalog) return
    const abort = new AbortController()
    void (async () => {
      const snapshot = await client.catalog?.(edition === "jp" ? "en" : "jp", abort.signal)
      if (snapshot && !abort.signal.aborted) setResult({ key, snapshot })
    })().catch((error: unknown) => {
      if (!abort.signal.aborted) setResult({ key, error })
    })
    return () => {
      abort.abort()
    }
  }, [client, edition, enabled, ready, key])
  const current = enabled && result?.key === key ? result : undefined
  return {
    snapshot: current?.snapshot,
    catalog: current?.snapshot ? catalogOf(current.snapshot) : undefined,
    error: current?.error,
    retry: () => {
      setAttempt((value) => value + 1)
    },
  }
}
