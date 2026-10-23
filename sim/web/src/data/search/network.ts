import type { Fetcher, FetchOptions } from "../cdn"
import { appRequests } from "../request-queue"

interface NetworkRequest {
  readonly kind: "fetch"
  readonly id: number
  readonly url: string
  readonly foreground: boolean
  readonly cache?: RequestCache
  readonly raw?: FetchOptions["raw"]
  readonly priority?: RequestPriority
  readonly speculative?: boolean
}
interface NetworkReply {
  readonly kind: "fetch-reply"
  readonly id: number
  readonly bytes?: ArrayBuffer
  readonly status?: number
  readonly headers?: readonly [string, string][]
  readonly error?: string
}
export type NetworkMessage =
  NetworkRequest | NetworkReply | { readonly kind: "abort-fetch"; readonly id: number }

export function workerFetch(
  post: (message: NetworkMessage) => void,
  foreground = true,
): {
  fetch: Fetcher
  receive: (message: NetworkReply) => void
} {
  let id = foreground ? 0 : 1
  const pending = new Map<
    number,
    { resolve: (response: Response) => void; reject: (error: unknown) => void; cleanup: () => void }
  >()
  return {
    fetch: (url, init) =>
      new Promise((resolve, reject) => {
        const next = (id += 2)
        const abort = () => {
          post({ kind: "abort-fetch", id: next })
          pending.delete(next)
          reject(new Error("request cancelled"))
        }
        if (init?.signal?.aborted) {
          reject(new Error("request cancelled"))
          return
        }
        init?.signal?.addEventListener("abort", abort, { once: true })
        pending.set(next, {
          resolve,
          reject,
          cleanup: () => init?.signal?.removeEventListener("abort", abort),
        })
        post({
          kind: "fetch",
          id: next,
          url,
          foreground,
          ...(init?.cache ? { cache: init.cache } : {}),
          ...(init?.raw ? { raw: init.raw } : {}),
          ...(init?.priority ? { priority: init.priority } : {}),
          ...(init?.speculative ? { speculative: true } : {}),
        })
      }),
    receive: (message) => {
      const job = pending.get(message.id)
      if (!job) return
      pending.delete(message.id)
      job.cleanup()
      if (message.error) job.reject(new Error(message.error))
      else
        job.resolve(
          new Response(message.bytes, {
            status: message.status ?? 200,
            headers: message.headers as [string, string][],
          }),
        )
    },
  }
}
export function mainNetwork(worker: Worker): {
  receive: (message: NetworkMessage) => void
  dispose: () => void
} {
  const pending = new Map<number, AbortController>()
  return {
    receive: (message) => {
      if (message.kind === "fetch-reply") return
      if (message.kind === "abort-fetch") {
        pending.get(message.id)?.abort()
        return
      }
      const abort = new AbortController()
      pending.set(message.id, abort)
      void (message.foreground ? appRequests.foreground : appRequests.background)(message.url, {
        signal: abort.signal,
        ...(message.cache ? { cache: message.cache } : {}),
        ...(message.raw ? { raw: message.raw } : {}),
        ...(message.priority ? { priority: message.priority } : {}),
        ...(message.speculative ? { speculative: true } : {}),
      })
        .then(async (response) => {
          const bytes = await response.arrayBuffer()
          worker.postMessage(
            {
              kind: "fetch-reply",
              id: message.id,
              bytes,
              status: response.status,
              headers: [...response.headers],
            },
            [bytes],
          )
        })
        .catch((error: unknown) => {
          worker.postMessage({
            kind: "fetch-reply",
            id: message.id,
            error: error instanceof Error ? error.message : "network failed",
          })
        })
        .finally(() => pending.delete(message.id))
    },
    dispose: () => {
      for (const abort of pending.values()) abort.abort()
      pending.clear()
    },
  }
}
