import type { Fetcher, FetchOptions } from "./cdn"

const SLOTS = 32
const BACKGROUND_SLOTS = 24
const SPECULATIVE_SLOTS = 2
interface Waiter {
  readonly resolve: (response: Response) => void
  readonly reject: (error: unknown) => void
  readonly cleanup: () => void
  readonly priority: number
}
interface Job {
  readonly key: string
  readonly url: string
  readonly init: FetchOptions | undefined
  readonly abort: AbortController
  readonly waiters: Set<Waiter>
  started: boolean
}
function priority(job: Job): number {
  return Math.min(...[...job.waiters].map((waiter) => waiter.priority))
}
/** One app budget holds permits through body reads and shares requests across all consumers. */
export function requestQueue(fetcher: Fetcher): {
  readonly foreground: Fetcher
  readonly background: Fetcher
} {
  const jobs = new Map<string, Job>()
  const queue: Job[] = []
  let running = 0
  let background = 0
  let speculative = 0
  const pump = () => {
    while (running < SLOTS) {
      const foregroundActive = [...jobs.values()].some((job) => priority(job) === 0)
      let index = -1
      let nextPriority = Infinity
      for (const [candidate, job] of queue.entries()) {
        const level = priority(job)
        if (level > 0 && background >= BACKGROUND_SLOTS) continue
        if (level === 3 && (speculative >= SPECULATIVE_SLOTS || foregroundActive)) continue
        if (level < nextPriority) {
          index = candidate
          nextPriority = level
        }
      }
      if (index < 0) return
      const job = queue.splice(index, 1)[0]
      if (!job) return
      job.started = true
      const isBackground = nextPriority > 0
      const isSpeculative = nextPriority === 3
      running += 1
      if (isBackground) background += 1
      if (isSpeculative) speculative += 1
      const { raw: _raw, speculative: _speculative, ...init } = job.init ?? {}
      void fetcher(job.url, {
        ...init,
        ...(nextPriority === 0 && init.priority === "low" ? { priority: "auto" } : {}),
        ...(isSpeculative ? { priority: "low" } : {}),
        signal: job.abort.signal,
      })
        .then(async (response) => {
          const bytes = await response.arrayBuffer()
          // Remove the completed identity before consumers can retry a corrupt response.
          if (jobs.get(job.key) === job) jobs.delete(job.key)
          for (const waiter of job.waiters)
            waiter.resolve(
              new Response(bytes, { status: response.status, headers: response.headers }),
            )
        })
        .catch((error: unknown) => {
          for (const waiter of job.waiters) waiter.reject(error)
        })
        .finally(() => {
          for (const waiter of job.waiters) waiter.cleanup()
          if (jobs.get(job.key) === job) jobs.delete(job.key)
          running -= 1
          if (isBackground) background -= 1
          if (isSpeculative) speculative -= 1
          pump()
        })
    }
  }
  const enqueue =
    (foreground: boolean): Fetcher =>
    (url, init) =>
      new Promise((resolve, reject) => {
        if (init?.signal?.aborted) {
          reject(new Error("request cancelled"))
          return
        }
        const key = JSON.stringify([
          init?.raw ? [init.raw.root, init.raw.sha256, init.raw.bytes] : url,
          init?.cache ?? "default",
        ])
        let job = jobs.get(key)
        if (!job || job.abort.signal.aborted) {
          job = { key, url, init, abort: new AbortController(), waiters: new Set(), started: false }
          jobs.set(key, job)
          queue.push(job)
        }
        const selected = job
        const abort = () => {
          selected.waiters.delete(waiter)
          waiter.cleanup()
          reject(new Error("request cancelled"))
          if (!selected.waiters.size) {
            selected.abort.abort()
            if (jobs.get(key) === selected) jobs.delete(key)
            if (!selected.started) {
              const index = queue.indexOf(selected)
              if (index >= 0) queue.splice(index, 1)
            }
          }
          pump()
        }
        const waiter: Waiter = {
          resolve,
          reject,
          priority: init?.speculative ? 3 : foreground ? 0 : init?.priority === "low" ? 2 : 1,
          cleanup: () => init?.signal?.removeEventListener("abort", abort),
        }
        selected.waiters.add(waiter)
        init?.signal?.addEventListener("abort", abort, { once: true })
        pump()
      })
  return { foreground: enqueue(true), background: enqueue(false) }
}
export const appRequests = requestQueue((url, init) => fetch(url, init))
