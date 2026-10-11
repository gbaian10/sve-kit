// @vitest-environment node
import { expect, it } from "vitest"

import { requestQueue } from "./request-queue"

it("shares body reads, promotes queued work, and keeps the app network within its total and background bounds", async () => {
  let active = 0
  let peak = 0
  const gates = new Map<string, () => void>()
  const started: string[] = []
  const queue = requestQueue((url) => {
    active += 1
    peak = Math.max(peak, active)
    started.push(url)
    const gate = Promise.withResolvers<undefined>()
    gates.set(url, () => {
      gate.resolve(undefined)
    })
    return Promise.resolve(
      new Response(
        new ReadableStream({
          async start(controller) {
            await gate.promise
            controller.enqueue(new Uint8Array([1]))
            controller.close()
            active -= 1
          },
        }),
      ),
    )
  })
  const jobs = Array.from({ length: 40 }, (_, n) => queue.background(`https://test/${String(n)}`))
  await Promise.resolve()
  expect(started).toHaveLength(24)
  const promoted = queue.foreground("https://test/39")
  const visible = Array.from({ length: 10 }, (_, n) =>
    queue.foreground(`https://test/visible-${String(n)}`),
  )
  await Promise.resolve()
  expect(started).toHaveLength(32)
  expect(started[24]).toBe("https://test/39")
  for (let round = 0; round < 6; round += 1) {
    for (const release of gates.values()) release()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
  await Promise.all([...jobs, promoted, ...visible])
  expect(peak).toBeLessThanOrEqual(32)
  expect(started.filter((url) => url === "https://test/39")).toHaveLength(1)
})

it("one consumer's cancellation leaves the shared download available to another", async () => {
  const gate = Promise.withResolvers<undefined>()
  let calls = 0
  const queue = requestQueue(async () => {
    calls += 1
    await gate.promise
    return new Response("valid")
  })
  const abort = new AbortController()
  const cancelled = queue.background("https://test/shared", { signal: abort.signal })
  const retained = queue.foreground("https://test/shared")
  abort.abort()
  await expect(cancelled).rejects.toThrow("cancelled")
  gate.resolve(undefined)
  expect(await (await retained).text()).toBe("valid")
  expect(calls).toBe(1)
})
