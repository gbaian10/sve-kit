import { decodeMessage, type DecodeRequest, type DecodeResult } from "./decode-message"
import { type ReaderErrorCode, SnapshotError } from "./errors"

/** A single sequential worker avoids one schema/decoder heap per downloaded bucket. */
export function createDecoder(): (request: DecodeRequest) => Promise<DecodeResult> {
  let worker: Worker | undefined
  let id = 0
  let idle: ReturnType<typeof setTimeout> | undefined
  const waiting = new Map<
    number,
    { resolve: (value: DecodeResult) => void; reject: (error: unknown) => void }
  >()
  return (request) => {
    if (typeof Worker === "undefined") return Promise.resolve().then(() => decodeMessage(request))
    clearTimeout(idle)
    worker ??= new Worker(new URL("./decode-worker.ts", import.meta.url), { type: "module" })
    worker.onmessage = (
      event: MessageEvent<{ id: number; result?: DecodeResult; code?: ReaderErrorCode }>,
    ) => {
      const pending = waiting.get(event.data.id)
      if (!pending) return
      waiting.delete(event.data.id)
      if (event.data.result) pending.resolve(event.data.result)
      else pending.reject(new SnapshotError(event.data.code ?? "shape", "worker validation failed"))
      // Idle workers must release their schema and parsing heap between page requests.
      if (waiting.size === 0)
        idle = setTimeout(() => {
          worker?.terminate()
          worker = undefined
        }, 1000)
    }
    worker.onerror = () => {
      for (const pending of waiting.values())
        pending.reject(new SnapshotError("shape", "decoder worker failed"))
      waiting.clear()
      worker?.terminate()
      worker = undefined
    }
    return new Promise((resolve, reject) => {
      const number = ++id
      waiting.set(number, { resolve, reject })
      worker?.postMessage({ id: number, request })
    })
  }
}
