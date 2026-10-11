// @vitest-environment node
import { expect, it } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { VIEW_BUDGET } from "./cache-budget"
import { VerifiedCache } from "./verified-cache"

it("keeps the latest complete view, bounds all view slots together, and isolates another root", async () => {
  const storage = memoryCache()
  const root = "https://cdn.test/root"
  const otherRoot = "https://cdn.test/root-other"
  const first = new VerifiedCache(root, "search-view", 1, storage, "test")
  const other = new VerifiedCache(otherRoot, "search-view", 1, storage, "test")
  const bytes = new Uint8Array(VIEW_BUDGET / 2)
  const hash = `sha256:${"1".repeat(64)}`
  await other.write("default", hash, bytes)
  for (const slot of ["first", "second", "third"]) await first.write(slot, hash, bytes)
  expect(await first.read("first", hash)).toBeUndefined()
  expect((await first.read("second", hash))?.length).toBe(bytes.length)
  expect((await first.read("third", hash))?.length).toBe(bytes.length)
  expect((await other.read("default", hash))?.length).toBe(bytes.length)
})

it("retains only two generations across distinct query slots", async () => {
  const storage = memoryCache()
  const cache = new VerifiedCache("https://cdn.test", "search-view", 1, storage, "test")
  for (const n of [1, 2, 3])
    await cache.write(`query-${String(n)}`, `sha256:${String(n).repeat(64)}`, new Uint8Array([n]))
  expect(await cache.read("query-1", `sha256:${"1".repeat(64)}`)).toBeUndefined()
  expect(await cache.read("query-2", `sha256:${"2".repeat(64)}`)).toEqual(new Uint8Array([2]))
  expect(await cache.read("query-3", `sha256:${"3".repeat(64)}`)).toEqual(new Uint8Array([3]))
})
