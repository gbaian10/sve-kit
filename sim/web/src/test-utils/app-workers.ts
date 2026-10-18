import { decodeMessage, type DecodeRequest, SnapshotError } from "../data"
import { TestSearchWorker } from "./search-worker"

/** The decode worker's protocol, run on the jsdom thread as separate tasks. */
class TestDecodeWorker {
  onmessage: ((event: MessageEvent<unknown>) => void) | null = null
  onerror: (() => void) | null = null
  private stopped = false
  postMessage(message: { id: number; request: DecodeRequest }): void {
    setTimeout(() => {
      if (this.stopped) return
      try {
        this.onmessage?.({
          data: { id: message.id, result: decodeMessage(message.request) },
        } as MessageEvent<unknown>)
      } catch (error) {
        this.onmessage?.({
          data: { id: message.id, code: error instanceof SnapshotError ? error.code : "shape" },
        } as MessageEvent<unknown>)
      }
    }, 0)
  }
  terminate(): void {
    this.stopped = true
  }
}

/**
 * Stands in for `Worker` when a page needs both the search engine and the main-thread snapshot
 * client, which decodes through its own worker; each is picked by the script it asks for.
 */
export function TestAppWorker(url: URL | string): TestDecodeWorker | TestSearchWorker {
  const script = typeof url === "string" ? url : url.href
  return script.includes("decode-worker") ? new TestDecodeWorker() : new TestSearchWorker()
}
