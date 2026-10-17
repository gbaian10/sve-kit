import { catalogInterval } from "./catalog-timing"

export function drainCatalog<T>(cursor: Iterator<void, T>): T {
  let step = cursor.next()
  while (!step.done) step = cursor.next()
  return step.value
}

/** One deadline spans validation and construction, including their microtask continuations. */
export class CatalogBudget {
  private deadline = performance.now() + 8
  private channel: MessageChannel | undefined

  private readonly check: () => void

  constructor(check: () => void) {
    this.check = check
  }

  private pause(): Promise<void> {
    const scheduler = (globalThis as unknown as { scheduler?: { yield?: () => Promise<void> } })
      .scheduler
    if (scheduler?.yield) return scheduler.yield()
    const channel = (this.channel ??= new MessageChannel())
    return new Promise((resolve) => {
      channel.port1.onmessage = () => {
        resolve()
      }
      channel.port2.postMessage(null)
    })
  }

  async drain<T>(cursor: Iterator<void, T>, stage: string): Promise<T> {
    this.check()
    let start = performance.now()
    let active = true
    const finish = (end: number): void => {
      if (active) {
        catalogInterval(stage, start, end)
        active = false
      }
    }
    try {
      for (;;) {
        const step = cursor.next()
        const now = performance.now()
        if (step.done) {
          finish(now)
          this.check()
          return step.value
        }
        if (now < this.deadline) continue
        finish(now)
        this.check()
        await this.pause()
        this.check()
        start = performance.now()
        this.deadline = start + 8
        active = true
      }
    } finally {
      finish(performance.now())
      cursor.return?.()
    }
  }

  close(): void {
    if (!this.channel) return
    this.channel.port1.onmessage = null
    this.channel.port1.close()
    this.channel.port2.close()
  }
}
