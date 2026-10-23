// @vitest-environment node
import { expect, it, vi } from "vitest"

import { requestQueue } from "./request-queue"

it("shares body reads, promotes queued work, and keeps the app network within its total and background bounds", async () => {
  let active = 0
  let peak = 0
  const gates = new Map<string, () => void>()
  const started: string[] = []
  const priorities = new Map<string, RequestPriority | undefined>()
  const queue = requestQueue((url, init) => {
    priorities.set(url, init?.priority)
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
  const jobs = Array.from({ length: 40 }, (_, n) =>
    queue.background(`https://test/${String(n)}`, n === 39 ? { priority: "low" } : undefined),
  )
  await Promise.resolve()
  expect(started).toHaveLength(24)
  const promoted = queue.foreground("https://test/39")
  const visible = Array.from({ length: 10 }, (_, n) =>
    queue.foreground(`https://test/visible-${String(n)}`),
  )
  await Promise.resolve()
  expect(started).toHaveLength(32)
  expect(started[24]).toBe("https://test/39")
  expect(priorities.get("https://test/39")).not.toBe("low")
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

it("copies the shared source into independently readable responses before publishing them", async () => {
  const source = new Uint8Array([1, 2, 3])
  const response = new Response(
    new ReadableStream({
      start(controller) {
        controller.enqueue(source)
        controller.close()
      },
    }),
    { status: 201, headers: { "x-shared": "yes" } },
  )
  const fetcher = vi.fn(() => Promise.resolve(response))
  const queue = requestQueue(fetcher)
  const [first, second] = await Promise.all([
    queue.foreground("https://test/shared-body"),
    queue.background("https://test/shared-body"),
  ])
  // The queue copies stream chunks before publishing, so later writes to the source stay invisible.
  source.fill(9)
  expect(new Uint8Array(await first.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]))
  expect(new Uint8Array(await second.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]))
  expect(first.status).toBe(201)
  expect(second.headers.get("x-shared")).toBe("yes")
  expect(fetcher).toHaveBeenCalledTimes(1)
})

it("limits speculative bodies app-wide, promotes queued subscribers, and pauses dispatch for P0", async () => {
  const gates = new Map<string, () => void>()
  const started: string[] = []
  const queue = requestQueue((url) => {
    let cancelled = false
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
            if (cancelled) return
            controller.enqueue(new Uint8Array([1]))
            controller.close()
          },
          cancel() {
            cancelled = true
          },
        }),
      ),
    )
  })
  const speculative = Array.from({ length: 5 }, (_, n) =>
    (n === 0 ? queue.foreground : queue.background)(`https://test/spec-${String(n)}`, {
      speculative: true,
      raw: { root: "test", sha256: String(n), bytes: 512 * 1024 },
    }),
  )
  expect(started).toHaveLength(2)
  const promoted = queue.foreground("https://test/spec-4", {
    raw: { root: "test", sha256: "4", bytes: 512 * 1024 },
  })
  expect(started).toEqual(["https://test/spec-0", "https://test/spec-1", "https://test/spec-4"])
  const background = Array.from({ length: 25 }, (_, n) =>
    queue.background(`https://test/print-${String(n)}`),
  )
  expect(started).toHaveLength(25)
  const visible = queue.foreground("https://test/visible")
  expect(started.at(-1)).toBe("https://test/visible")
  gates.get("https://test/spec-0")?.()
  gates.get("https://test/spec-1")?.()
  await new Promise((resolve) => setTimeout(resolve, 0))
  expect(started).not.toContain("https://test/spec-2")
  for (let round = 0; round < 6; round += 1) {
    for (const release of gates.values()) release()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
  await Promise.all([...speculative, ...background, promoted, visible])
  expect(started.filter((url) => url === "https://test/spec-4")).toHaveLength(1)
})

it("interrupts a stalled speculative body for P0 while preserving a shared foreground body and retrying later", async () => {
  const starts = new Map<string, number>()
  const finish = new Map<string, () => void>()
  const cancelled: string[] = []
  const queue = requestQueue((url) => {
    const attempt = (starts.get(url) ?? 0) + 1
    starts.set(url, attempt)
    if (url === "visible" || attempt > 1) return Promise.resolve(new Response("ready"))
    return Promise.resolve(
      new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(new TextEncoder().encode("partial"))
            finish.set(url, () => {
              controller.close()
            })
          },
          cancel() {
            cancelled.push(url)
          },
        }),
      ),
    )
  })
  const a = queue.background("slow-a", {
    speculative: true,
    raw: { root: "test", sha256: "a", bytes: 512 * 1024 },
  })
  const b = queue.background("slow-b", {
    speculative: true,
    raw: { root: "test", sha256: "b", bytes: 512 * 1024 },
  })
  await new Promise((resolve) => setTimeout(resolve, 0))
  const shared = queue.foreground("slow-b", {
    raw: { root: "test", sha256: "b", bytes: 512 * 1024 },
  })
  const rejoined = queue.foreground("slow-a", {
    raw: { root: "test", sha256: "a", bytes: 512 * 1024 },
  })
  expect(await (await queue.foreground("visible")).text()).toBe("ready")
  expect(cancelled).toEqual(["slow-a"])
  expect(await (await rejoined).text()).toBe("ready")
  finish.get("slow-b")?.()
  expect(await (await shared).text()).toBe("partial")
  expect(await (await b).text()).toBe("partial")
  expect(await (await a).text()).toBe("ready")
  expect(starts.get("slow-a")).toBe(2)
  expect(starts.get("slow-b")).toBe(1)
})

it("releases aggregate speculative byte credit after completion and cancellation and admits an oversized catalog alone", async () => {
  const finish = new Map<string, () => void>()
  const started: string[] = []
  const cancelled: string[] = []
  const queue = requestQueue((url) => {
    started.push(url)
    return Promise.resolve(
      new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(new Uint8Array([1]))
            finish.set(url, () => {
              controller.close()
            })
          },
          cancel() {
            cancelled.push(url)
          },
        }),
      ),
    )
  })
  const abort = new AbortController()
  const read = (url: string, bytes: number, signal?: AbortSignal) =>
    queue.background(url, {
      speculative: true,
      raw: { root: "test", sha256: url, bytes },
      ...(signal ? { signal } : {}),
    })
  const large = read("large", 768 * 1024, abort.signal)
  const rejected = expect(large).rejects.toThrow("cancelled")
  const medium = read("medium", 512 * 1024)
  const small = read("small", 256 * 1024)
  expect(started).toEqual(["large", "small"])
  finish.get("small")?.()
  await small
  await new Promise((resolve) => setTimeout(resolve, 0))
  expect(started).not.toContain("medium")
  abort.abort()
  await rejected
  await new Promise((resolve) => setTimeout(resolve, 0))
  expect(cancelled).toContain("large")
  expect(started).toContain("medium")
  const catalog = read("catalog", 2 * 1024 * 1024)
  expect(started).not.toContain("catalog")
  finish.get("medium")?.()
  await medium
  await new Promise((resolve) => setTimeout(resolve, 0))
  expect(started.at(-1)).toBe("catalog")
  const next = read("next", 128 * 1024)
  expect(started).not.toContain("next")
  finish.get("catalog")?.()
  await catalog
  await new Promise((resolve) => setTimeout(resolve, 0))
  expect(started.at(-1)).toBe("next")
  finish.get("next")?.()
  await next
})
