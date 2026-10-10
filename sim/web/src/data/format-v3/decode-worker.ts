import { decodeMessage, type DecodeRequest } from "./decode-message"
import { SnapshotError } from "./errors"

self.addEventListener("message", (event: MessageEvent<{ id: number; request: DecodeRequest }>) => {
  try {
    self.postMessage({ id: event.data.id, result: decodeMessage(event.data.request) })
  } catch (error) {
    self.postMessage({
      id: event.data.id,
      code: error instanceof SnapshotError ? error.code : "shape",
    })
  }
})
