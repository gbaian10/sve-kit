import type { CDPSession, Page } from "playwright"

export interface Measure {
  realm: string
  name: string
  startMs: number
  durationMs: number
}
export interface MemorySample {
  startMs: number
  spanMs: number
  bytes: number
  realms: number
  complete: boolean
  isolates: { realm: string; bytes: number }[]
}
interface HeapUsage {
  usedSize: number
  backingStorageSize: number
}
interface Reply {
  id: number
  result?: unknown
  error?: { message: string }
}

// Binding delivery survives decoder-worker termination; reading its timeline afterwards cannot.
export const CATALOG_OBSERVER = `(() => {
  const state = globalThis.catalogBenchObserverState ??= { pending: [] };
  const flush = () => {
    if (typeof globalThis.catalogBenchRecord !== 'function') return;
    for (const entry of state.pending.splice(0))
      globalThis.catalogBenchRecord(JSON.stringify(entry));
  };
  if (!state.observer) {
    state.observer = new PerformanceObserver(list => {
      for (const entry of list.getEntries()) {
        if (entry.entryType === 'longtask' || entry.name.startsWith('sve:catalog:'))
          state.pending.push({ name: entry.name, type: entry.entryType,
            startMs: performance.timeOrigin + entry.startTime, durationMs: entry.duration });
      }
      flush();
    });
    state.observer.observe({entryTypes: typeof document === 'undefined' ? ['measure'] : ['measure', 'longtask']});
  }
  flush();
})();`

type Pending = {
  sessionId: string
  timer: ReturnType<typeof setTimeout>
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

export class CatalogMonitor {
  readonly measures: Measure[] = []
  readonly tasks: Measure[] = []
  readonly memory: MemorySample[] = []
  readonly errors: string[] = []
  private readonly workers = new Map<string, string>()
  private readonly pending = new Map<number, Pending>()
  private sequence = 0
  private workerSequence = 0
  private workerEpoch = 0
  private polling: Promise<void> | undefined
  private stopped = false
  private readonly main: CDPSession

  private constructor(main: CDPSession) {
    this.main = main
  }

  static async create(page: Page): Promise<CatalogMonitor> {
    const main = await page.context().newCDPSession(page)
    const monitor = new CatalogMonitor(main)
    main.on("Runtime.bindingCalled", ({ name, payload }: { name: string; payload: string }) => {
      if (name === "catalogBenchRecord") monitor.record("main", payload)
    })
    main.on(
      "Target.receivedMessageFromTarget",
      ({ sessionId, message }: { sessionId: string; message: string }) => {
        const reply = JSON.parse(message) as Reply & {
          method?: string
          params?: { name: string; payload: string }
        }
        if (reply.id) {
          const pending = monitor.pending.get(reply.id)
          monitor.pending.delete(reply.id)
          if (pending) clearTimeout(pending.timer)
          if (reply.error) pending?.reject(new Error(reply.error.message))
          else pending?.resolve(reply.result)
        } else if (
          reply.method === "Runtime.bindingCalled" &&
          reply.params?.name === "catalogBenchRecord"
        ) {
          monitor.record(monitor.workers.get(sessionId) ?? "worker", reply.params.payload)
        }
      },
    )
    main.on(
      "Target.attachedToTarget",
      ({
        sessionId,
        targetInfo,
      }: {
        sessionId: string
        targetInfo: { type: string; url: string }
      }) => {
        const realm = `${targetInfo.type}:${String(++monitor.workerSequence)}:${new URL(targetInfo.url, "http://localhost").pathname.split("/").at(-1) ?? "unknown"}`
        monitor.workers.set(sessionId, realm)
        monitor.workerEpoch += 1
        void (async () => {
          try {
            await monitor.send(sessionId, "Runtime.enable")
            await monitor.send(sessionId, "Runtime.addBinding", { name: "catalogBenchRecord" })
            await monitor.send(sessionId, "Runtime.evaluate", { expression: CATALOG_OBSERVER })
          } catch (error) {
            monitor.errors.push(String(error))
          } finally {
            await monitor.send(sessionId, "Runtime.runIfWaitingForDebugger")
          }
        })().catch((error: unknown) => monitor.errors.push(String(error)))
      },
    )
    main.on("Target.detachedFromTarget", ({ sessionId }: { sessionId: string }) => {
      monitor.workers.delete(sessionId)
      monitor.workerEpoch += 1
      for (const [id, pending] of monitor.pending) {
        if (pending.sessionId !== sessionId) continue
        clearTimeout(pending.timer)
        pending.reject(new Error("Worker detached during CDP request"))
        monitor.pending.delete(id)
      }
    })
    await main.send("Runtime.enable")
    await main.send("Page.enable")
    await main.send("Runtime.addBinding", { name: "catalogBenchRecord" })
    await main.send("Page.addScriptToEvaluateOnNewDocument", { source: CATALOG_OBSERVER })
    await main.send("Target.setAutoAttach", {
      autoAttach: true,
      waitForDebuggerOnStart: true,
      flatten: false,
      filter: [{ type: "worker", exclude: false }, { exclude: true }],
    })
    return monitor
  }

  private record(realm: string, payload: string): void {
    const entry = JSON.parse(payload) as Omit<Measure, "realm"> & { type: string }
    const { type, ...measure } = entry
    ;(type === "longtask" ? this.tasks : this.measures).push({ realm, ...measure })
  }

  private send(sessionId: string, method: string, params: object = {}): Promise<unknown> {
    const id = ++this.sequence
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`Worker CDP request timed out: ${method}`))
      }, 10_000)
      this.pending.set(id, { sessionId, timer, resolve, reject })
      void this.main
        .send("Target.sendMessageToTarget", {
          sessionId,
          message: JSON.stringify({ id, method, params }),
        })
        .catch((error: unknown) => {
          this.pending.delete(id)
          clearTimeout(timer)
          reject(error instanceof Error ? error : new Error(String(error)))
        })
    })
  }

  async sample(): Promise<void> {
    const startMs = Date.now()
    const epoch = this.workerEpoch
    const workers = [...this.workers]
    const replies = await Promise.allSettled([
      this.main.send("Runtime.getHeapUsage"),
      ...workers.map(([id]) => this.send(id, "Runtime.getHeapUsage")),
    ])
    const isolates: { realm: string; bytes: number }[] = []
    for (const [ordinal, reply] of replies.entries()) {
      if (reply.status === "rejected") continue
      const usage = reply.value as HeapUsage
      if (!Number.isFinite(usage.backingStorageSize))
        throw new Error("Chromium must support Runtime.getHeapUsage.backingStorageSize")
      isolates.push({
        realm: ordinal === 0 ? "main" : (workers[ordinal - 1]?.[1] ?? "worker"),
        bytes: usage.usedSize + usage.backingStorageSize,
      })
    }
    this.memory.push({
      startMs,
      spanMs: Date.now() - startMs,
      bytes: isolates.reduce((sum, isolate) => sum + isolate.bytes, 0),
      realms: isolates.length,
      complete: epoch === this.workerEpoch && isolates.length === replies.length,
      isolates,
    })
  }

  start(intervalMs: number): void {
    this.polling = (async () => {
      while (!this.stopped) {
        await this.sample()
        await new Promise((resolve) => setTimeout(resolve, intervalMs))
      }
    })()
  }

  async stop(): Promise<void> {
    this.stopped = true
    await this.polling
    if (this.errors.length) throw new Error(this.errors.join("\n"))
  }
}
