// @vitest-environment node
import { PerformanceObserver } from "node:perf_hooks"

import { expect, it } from "vitest"

import { catalogWork } from "./catalog-timing"

it("preserves results and errors while delivering bounded User Timing observations", async () => {
  const entries = new Promise<string[]>((resolve) => {
    const observer = new PerformanceObserver((list) => {
      observer.disconnect()
      resolve(list.getEntries().map((entry) => entry.name))
    })
    observer.observe({ entryTypes: ["measure"] })
  })
  const result = {}
  expect(catalogWork("test:result", () => result)).toBe(result)
  const error = new Error("synthetic failure")
  expect(() =>
    catalogWork("test:error", () => {
      throw error
    }),
  ).toThrow(error)
  expect(await entries).toEqual(["sve:catalog:test:result", "sve:catalog:test:error"])
  expect(
    performance.getEntries().filter((entry) => entry.name.startsWith("sve:catalog:test:")),
  ).toEqual([])
})
