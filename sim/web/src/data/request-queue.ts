import type { Fetcher } from "./cdn"

// Real networks add hundreds of milliseconds per request, so startup needs several in flight;
// background work never takes the last two slots, which stay free for visible work.
const SLOTS = 8
const BACKGROUND_SLOTS = 6

/** One snapshot's network budget; some slots are reserved for visible work. */
export function requestQueue(fetcher: Fetcher): {
  readonly foreground: Fetcher
  readonly background: Fetcher
} {
  const queue: { priority: boolean; run: () => void }[] = []
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
      job.run()
    }
  }
  const enqueue =
    (priority: boolean): Fetcher =>
    (url, init) =>
      new Promise((resolve, reject) => {
        queue.push({
          priority,
          run: () => {
            if (init?.signal?.aborted) {
              reject(new Error("request cancelled"))
              return
            }
            running += 1
            if (!priority) background += 1
            // Reading the body is part of the same slot, rather than only waiting for headers.
            void fetcher(url, init)
              .then(async (response) => {
                const bytes = await response.arrayBuffer()
                return new Response(bytes, { status: response.status, headers: response.headers })
              })
              .then(resolve, reject)
              .finally(() => {
                running -= 1
                if (!priority) background -= 1
                pump()
              })
          },
        })
        pump()
      })
  return { foreground: enqueue(true), background: enqueue(false) }
}
