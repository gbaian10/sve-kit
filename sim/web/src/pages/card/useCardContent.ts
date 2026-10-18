import { useCallback, useEffect, useRef, useState } from "react"

import { type CardContent, cardContentKey, prepareCardContent } from "../../app/card-content"
import { prefetchCards, type PrefetchTarget, summaryRoute } from "../../app/card-prefetch"
import type { CardEntryState } from "../../app/listEntryState"
import { useCardSnapshot } from "../../app/snapshot"
import { useTextLanguage } from "../../components/card/text-language"
import { type CardRouteInput, locateCard } from "../../data"
import type { Region } from "../../domain/search"
import type { CardViewProps } from "./CardPage"

const IMAGE_SIZES =
  "(min-width: 1440px) 340px, (min-width: 1000px) 280px, (min-width: 600px) 340px, 72vw"
// Each prepared card keeps its own catalog and text closure (several MB), so only nearby cards stay.
const PREPARED_CARDS = 4

/** The route hint a list or link handed over, as the card route input wants it. */
export function routeInput(
  namespace: CardRouteInput["namespace"],
  key: string,
  edition: Region,
  route: CardEntryState["route"] | undefined,
): CardRouteInput {
  return {
    namespace,
    key,
    edition,
    ...(route ? { hint: { ...route, id: route.printingId } } : {}),
  }
}

/**
 * Locates and prepares one card (text closure, images) for a route input, keeping a few nearby
 * cards ready; the page and the list's overlay both read the card through this one path.
 */
export function useCardContent(
  input: CardRouteInput,
): Pick<
  CardViewProps,
  | "client"
  | "preparedImages"
  | "onPrefetch"
  | "pending"
  | "showPending"
  | "status"
  | "catalog"
  | "resolved"
  | "routeError"
  | "onRetry"
> {
  const { client, status } = useCardSnapshot()
  const [attempt, setAttempt] = useState(0)
  const language = useTextLanguage()
  const [prepared, setPrepared] = useState(() => new Map<string, CardContent>())
  const preparedRef = useRef(prepared)
  const visited = useRef(new Set<string>())
  const completedEditions = useRef(new Set<string>())
  useEffect(() => {
    preparedRef.current = prepared
  }, [prepared])
  const inputKey = cardContentKey(client, input, language, attempt)
  const cached = prepared.get(inputKey)
  const [located, setLocated] = useState<{
    key: string
    value?: CardContent
    error?: unknown
  } | null>(null)
  if (cached && located?.key !== inputKey) setLocated({ key: inputKey, value: cached })
  const manifestHash = client.snapshot()?.manifestHash
  useEffect(() => {
    if (!manifestHash || cached) return
    let active = true
    const loading = locateCard(client, input).then((value) =>
      prepareCardContent(client, value, language, IMAGE_SIZES),
    )
    void loading.then(
      (value) => {
        if (active) {
          // A failed image index stays uncached so returning to the card retries it.
          if (!value.images.failed)
            setPrepared((previous) => {
              const next = new Map(previous).set(inputKey, value)
              if (next.size > PREPARED_CARDS) next.delete(next.keys().next().value ?? "")
              return next
            })
          setLocated({ key: inputKey, value })
        }
      },
      (error: unknown) => {
        if (active) setLocated({ key: inputKey, error })
      },
    )
    return () => {
      active = false
    }
  }, [client, manifestHash, inputKey, input, language, cached])
  const current = cached !== undefined || located?.key === inputKey
  const result = cached
    ? { key: inputKey, value: cached }
    : current ||
        (located?.value?.snapshot.manifestHash === manifestHash &&
          located?.value?.client.base === client.base)
      ? located
      : null
  const pending = !current
  const [pendingHint, setPendingHint] = useState<string | null>(null)
  useEffect(() => {
    if (!pending) return
    const timer = setTimeout(() => {
      setPendingHint(inputKey)
    }, 200)
    return () => {
      clearTimeout(timer)
    }
  }, [pending, inputKey])
  useEffect(() => {
    if (current && result?.value) visited.current.add(inputKey)
    for (const key of visited.current) if (!prepared.has(key)) visited.current.delete(key)
  }, [current, result?.value, inputKey, prepared])
  const onPrefetch = useCallback(
    (targets: readonly PrefetchTarget[], signal: AbortSignal) => {
      const protectedKeys = new Set([
        inputKey,
        ...targets
          .filter((target) => target.neighbour)
          .map(({ summary }) => cardContentKey(client, summaryRoute(summary), language, attempt)),
      ])
      return prefetchCards(
        client,
        targets.filter(({ summary, neighbour }) => {
          const key = cardContentKey(client, summaryRoute(summary), language, attempt)
          return !preparedRef.current.has(key) && (neighbour || !completedEditions.current.has(key))
        }),
        language,
        IMAGE_SIZES,
        signal,
        (target, value) => {
          if (signal.aborted || client.snapshot()?.manifestHash !== manifestHash) return
          const key = cardContentKey(client, target, language, attempt)
          completedEditions.current.add(key)
          setPrepared((previous) => {
            if (signal.aborted) return previous
            const next = new Map(previous).set(key, value)
            while (next.size > PREPARED_CARDS) {
              const candidates = [...next.keys()].filter(
                (candidate) => !protectedKeys.has(candidate),
              )
              const oldest =
                candidates.find((candidate) => !visited.current.has(candidate)) ?? candidates[0]
              if (oldest === undefined) break
              next.delete(oldest)
            }
            return next.has(key) ? next : previous
          })
        },
      )
    },
    [client, inputKey, language, attempt, manifestHash],
  )
  const onRetry = useCallback(() => {
    void client.reload().then(() => {
      setAttempt((value) => value + 1)
    })
  }, [client])
  return {
    client: result?.value?.client ?? client,
    preparedImages: result?.value?.images,
    onPrefetch,
    pending,
    showPending: pendingHint === inputKey,
    status,
    catalog: result?.value?.catalog ?? null,
    resolved: result?.value?.printing,
    routeError: result?.error,
    onRetry,
  }
}
