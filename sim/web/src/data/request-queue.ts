import type { Fetcher } from "./cdn"

/** One snapshot's network budget; one slot is reserved for visible work. */
export function requestQueue(fetcher: Fetcher): {
  readonly foreground: Fetcher
  readonly background: Fetcher
} {
  const queue: { priority: boolean; run: () => void }[] = []
  let running = 0
  let background = 0
  const pump = () => {
    while (running < 4) {
      let index = queue.findIndex((job) => job.priority)
      if (index < 0) {
        if (background >= 3) return
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
