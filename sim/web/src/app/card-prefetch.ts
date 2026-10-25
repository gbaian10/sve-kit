import PQueue from "p-queue"

import { type CardRouteInput, type CardSummary, locateCard, type SnapshotClient } from "../data"
import { type CardContent, prepareCardContent, yieldCardPreload } from "./card-content"

// Both sources share a budget; each card reads its fragments and images sequentially.
const queue = new PQueue({ concurrency: 2 })

export interface PrefetchTarget {
  readonly summary: CardSummary
  readonly neighbour: boolean
}

export function summaryRoute(summary: CardSummary): CardRouteInput {
  return {
    namespace: summary.cardNoState === "provisional" ? "provisional" : "official",
    key: summary.cardNoState === "provisional" ? String(summary.intId) : summary.cardNo,
    edition: summary.region,
    ...(summary.route
      ? {
          hint: {
            ...summary.route,
            id: summary.printingId,
            cardId: summary.cardId,
            region: summary.region,
          },
        }
      : {}),
  }
}

/** Prepare only this page's editions and immediate neighbours through the normal readers. */
export async function prefetchCards(
  client: SnapshotClient,
  targets: readonly PrefetchTarget[],
  language: string,
  signal: AbortSignal,
  accept: (input: CardRouteInput, value: CardContent) => void,
): Promise<void> {
  const seen = new Set<string>()
  await Promise.all(
    targets
      .filter(({ summary }) => {
        if (seen.has(summary.printingId)) return false
        seen.add(summary.printingId)
        return true
      })
      .map(({ summary, neighbour }) =>
        queue
          .add(
            async () => {
              await yieldCardPreload(signal)
              let snapshot = client.snapshot()
              if (!snapshot) return
              const hash = snapshot.manifestHash
              if (snapshot.region !== summary.region) {
                snapshot = (await client.catalog?.(summary.region, signal)) ?? null
                if (!snapshot || snapshot.manifestHash !== hash) return
              }
              signal.throwIfAborted()
              // A prepared resolver outlives its prefetch session; successful pages resume normal reads.
              let preparing = true
              const reads = new PQueue({ concurrency: 1 })
              const scoped: SnapshotClient = {
                ...client,
                snapshot: () => snapshot,
                load: () => Promise.resolve(),
                selectRegion: () => Promise.resolve(),
                fragments: async (key) => {
                  if (!preparing) return client.fragments(key)
                  signal.throwIfAborted()
                  const result = await reads.add(() => client.fragments(key, signal), { signal })
                  await yieldCardPreload(signal)
                  return result
                },
              }
              const input = summaryRoute(summary)
              const located = await locateCard(scoped, input)
              signal.throwIfAborted()
              const value = await prepareCardContent(scoped, located, language, signal)
              signal.throwIfAborted()
              if (client.snapshot()?.manifestHash !== hash || value.images.failed) return
              preparing = false
              accept(input, value)
            },
            { signal, priority: neighbour ? 1 : 0 },
          )
          .catch(() => undefined),
      ),
  )
}
