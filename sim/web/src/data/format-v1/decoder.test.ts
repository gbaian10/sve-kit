// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest"

import { decodeMessage, type DecodeRequest, type DecodeResult } from "./decode-message"
import { createDecoder } from "./decoder"
import { type ReaderErrorCode, SnapshotError } from "./errors"
import { canonical } from "./json"

class FakeWorker {
  static instances: FakeWorker[] = []
  onmessage?: (event: {
    data: { id: number; result?: DecodeResult; code?: ReaderErrorCode }
  }) => void
  onerror?: () => void
  terminated = false
  constructor() {
    FakeWorker.instances.push(this)
  }
  postMessage(message: { id: number; request: DecodeRequest }) {
    queueMicrotask(() => {
      try {
        this.onmessage?.({ data: { id: message.id, result: decodeMessage(message.request) } })
      } catch (error) {
        this.onmessage?.({
          data: { id: message.id, code: error instanceof SnapshotError ? error.code : "shape" },
        })
      }
    })
  }
  terminate() {
    this.terminated = true
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
  FakeWorker.instances = []
})
describe("decoder worker boundary", () => {
  it("shares one worker for concurrent files, propagates validation errors, then releases idle heap", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] })
    vi.stubGlobal("Worker", FakeWorker)
    const decode = createDecoder()
    await Promise.all(
      [1, 2].map(async () => {
        await expect(decode({ kind: "manifest", bytes: canonical({}) })).rejects.toThrow("shape")
      }),
    )
    expect(FakeWorker.instances).toHaveLength(1)
    expect(FakeWorker.instances[0]?.terminated).toBe(false)
    await vi.advanceTimersByTimeAsync(1000)
    expect(FakeWorker.instances[0]?.terminated).toBe(true)
  })
  it("the workerless fixture path fails for the same schema reason", async () => {
    vi.stubGlobal("Worker", undefined)
    await expect(createDecoder()({ kind: "manifest", bytes: canonical({}) })).rejects.toThrow(
      "shape",
    )
  })
})
