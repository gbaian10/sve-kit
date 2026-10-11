import type { Fetcher } from "./cdn"

const SLOTS = 32
const BACKGROUND_SLOTS = 24
interface Waiter {
  readonly resolve: (response: Response) => void
  readonly reject: (error: unknown) => void
  readonly cleanup: () => void
}
interface Job {
  readonly key: string
  readonly url: string
  readonly init: RequestInit | undefined
  readonly abort: AbortController
  readonly waiters: Set<Waiter>
  priority: boolean
  started: boolean
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
  const pump = () => {
    while (running < SLOTS) {
      let index = queue.findIndex((job) => job.priority)
      if (index < 0) {
        if (background >= BACKGROUND_SLOTS) return
        index = 0
      }
      const job = queue.splice(index, 1)[0]
      if (!job) return
      if (!job.waiters.size) continue
      job.started = true
      const isBackground = !job.priority
      running += 1
      if (isBackground) background += 1
      void fetcher(job.url, { ...job.init, signal: job.abort.signal })
        .then(async (response) => {
          const bytes = await response.arrayBuffer()
          for (const waiter of job.waiters)
            waiter.resolve(
              new Response(bytes.slice(0), { status: response.status, headers: response.headers }),
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
          pump()
        })
    }
  }
  const enqueue =
    (priority: boolean): Fetcher =>
    (url, init) =>
      new Promise((resolve, reject) => {
        if (init?.signal?.aborted) {
          reject(new Error("request cancelled"))
          return
        }
        const key = JSON.stringify([url, init?.cache ?? "default"])
        let job = jobs.get(key)
        if (!job || job.abort.signal.aborted) {
          job = {
            key,
            url,
            init,
            abort: new AbortController(),
            waiters: new Set(),
            priority,
            started: false,
          }
          jobs.set(key, job)
          queue.push(job)
        }
        if (priority) job.priority = true
        const selected = job
        const abort = () => {
          selected.waiters.delete(waiter)
          waiter.cleanup()
          reject(new Error("request cancelled"))
          if (!selected.waiters.size) {
            selected.abort.abort()
            if (jobs.get(key) === selected) jobs.delete(key)
          }
          pump()
        }
        const waiter: Waiter = {
          resolve,
          reject,
          cleanup: () => init?.signal?.removeEventListener("abort", abort),
        }
        selected.waiters.add(waiter)
        init?.signal?.addEventListener("abort", abort, { once: true })
        pump()
      })
  return { foreground: enqueue(true), background: enqueue(false) }
}
export const appRequests = requestQueue((url, init) => fetch(url, init))
