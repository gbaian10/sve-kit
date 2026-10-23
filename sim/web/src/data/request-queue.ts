import type { Fetcher, FetchOptions } from "./cdn"

const SLOTS = 32
const BACKGROUND_SLOTS = 24
const SPECULATIVE_SLOTS = 2
const BYTE_CREDIT = 1024 * 1024
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
  abort: AbortController
  readonly waiters: Set<Waiter>
  started: boolean
  preempted: boolean
}
function priority(job: Job): number {
  return Math.min(...[...job.waiters].map((waiter) => waiter.priority))
}
function credit(job: Job): number {
  // A larger catalog occupies the whole budget alone; unknown lengths cannot share credit.
  return Math.min(job.init?.raw?.bytes ?? BYTE_CREDIT, BYTE_CREDIT)
}
async function readBody(response: Response, signal: AbortSignal): Promise<ArrayBuffer> {
  if (!response.body) return new ArrayBuffer(0)
  const reader = response.body.getReader()
  const abort = () => {
    void reader.cancel(signal.reason).catch(() => undefined)
  }
  signal.addEventListener("abort", abort, { once: true })
  if (signal.aborted) abort()
  try {
    const chunks: Uint8Array[] = []
    let length = 0
    for (;;) {
      signal.throwIfAborted()
      const chunk = await reader.read()
      signal.throwIfAborted()
      if (chunk.done) break
      chunks.push(chunk.value)
      length += chunk.value.length
    }
    const bytes = new Uint8Array(length)
    let offset = 0
    for (const chunk of chunks) {
      bytes.set(chunk, offset)
      offset += chunk.length
    }
    return bytes.buffer
  } finally {
    signal.removeEventListener("abort", abort)
    reader.releaseLock()
  }
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
  let bytes = 0
  const pump = () => {
    const foregroundActive = [...jobs.values()].some((job) => priority(job) === 0)
    if (foregroundActive) {
      for (const job of jobs.values()) {
        if (!job.started || priority(job) !== 3 || job.preempted) continue
        job.preempted = true
        job.abort.abort(new Error("background download interrupted"))
      }
    }
    while (running < SLOTS) {
      let index = -1
      let nextPriority = Infinity
      for (const [candidate, job] of queue.entries()) {
        const level = priority(job)
        if (level > 0 && background >= BACKGROUND_SLOTS) continue
        if (
          level === 3 &&
          (speculative >= SPECULATIVE_SLOTS ||
            foregroundActive ||
            bytes + credit(job) > BYTE_CREDIT)
        )
          continue
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
      const reserved = isSpeculative ? credit(job) : 0
      const signal = job.abort.signal
      running += 1
      if (isBackground) background += 1
      if (isSpeculative) speculative += 1
      bytes += reserved
      const { raw: _raw, speculative: _speculative, ...init } = job.init ?? {}
      void (async () => {
        let retry = false
        try {
          const response = await fetcher(job.url, {
            ...init,
            ...(nextPriority === 0 && init.priority === "low" ? { priority: "auto" } : {}),
            ...(isSpeculative ? { priority: "low" } : {}),
            signal,
          })
          const body = await readBody(response, signal)
          signal.throwIfAborted()
          // Remove the completed identity before consumers can retry a corrupt response.
          if (jobs.get(job.key) === job) jobs.delete(job.key)
          for (const waiter of job.waiters)
            waiter.resolve(
              new Response(body, { status: response.status, headers: response.headers }),
            )
        } catch (error) {
          retry = job.preempted && job.waiters.size > 0
          if (!retry) {
            if (jobs.get(job.key) === job) jobs.delete(job.key)
            for (const waiter of job.waiters) waiter.reject(error)
          }
        } finally {
          running -= 1
          if (isBackground) background -= 1
          if (isSpeculative) speculative -= 1
          bytes -= reserved
          if (retry && jobs.get(job.key) === job) {
            job.started = false
            job.preempted = false
            job.abort = new AbortController()
            queue.push(job)
          } else {
            for (const waiter of job.waiters) waiter.cleanup()
          }
          pump()
        }
      })()
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
        if (!job || (job.abort.signal.aborted && !job.preempted)) {
          job = {
            key,
            url,
            init,
            abort: new AbortController(),
            waiters: new Set(),
            started: false,
            preempted: false,
          }
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
