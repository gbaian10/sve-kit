// @vitest-environment node
import { expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { cachedImage, versionedImage } from "./image-cache"

function opaqueResponse(): Response {
  const response = Response.error()
  Object.defineProperties(response, {
    type: { value: "opaque" },
    clone: { value: opaqueResponse },
  })
  return response
}

it("never stores opaque responses: each request reaches the network", async () => {
  const storage = memoryCache()
  const request = new Request("https://cdn.test/images/card_l/1.webp?v=2")
  let requests = 0
  const network: typeof fetch = () => {
    requests += 1
    return Promise.resolve(opaqueResponse())
  }
  expect((await cachedImage(request, storage, network)).type).toBe("opaque")
  expect((await cachedImage(request, storage, network)).type).toBe("opaque")
  expect(requests).toBe(2)
  expect(await (await storage.open("sve-image-responses-v2")).keys()).toHaveLength(0)
})
it("discards an older opaque cache hit before fetching instead of retaining padded quota", async () => {
  const storage = memoryCache()
  const cache = await storage.open("sve-image-responses-v2")
  const request = new Request("https://cdn.test/images/card_l/1.webp?v=2")
  await cache.put(request, opaqueResponse())
  let requests = 0
  const response = await cachedImage(request, storage, () => {
    requests += 1
    return Promise.resolve(opaqueResponse())
  })
  expect(response.type).toBe("opaque")
  expect(requests).toBe(1)
  expect(await cache.keys()).toHaveLength(0)
})

it("matches the complete versioned URL: old cached bytes never satisfy a new v or failed v", async () => {
  const storage = memoryCache()
  let bytes = "A"
  let failed = false
  const urls: string[] = []
  const network: typeof fetch = (input) => {
    urls.push((input as Request).url)
    return failed ? Promise.reject(new TypeError("offline")) : Promise.resolve(new Response(bytes))
  }
  const request = (v: number) =>
    new Request(`https://cdn.test/images/card_l/20001.webp?v=${String(v)}`)
  expect(await (await cachedImage(request(1), storage, network)).text()).toBe("A")
  bytes = "B"
  expect(await (await cachedImage(request(2), storage, network)).text()).toBe("B")
  expect(urls).toHaveLength(2)
  failed = true
  await expect(cachedImage(request(3), storage, network)).rejects.toThrow("offline")
  expect(await (await cachedImage(request(2), storage, network)).text()).toBe("B")
})
it("does not cache negative responses, and tolerates storage failure without an old-image fallback", async () => {
  const storage = memoryCache()
  let status = 404
  const request = new Request("https://cdn.test/images/card_l/1.webp?v=2")
  const fetcher: typeof fetch = () => Promise.resolve(new Response("synthetic", { status }))
  expect((await cachedImage(request, storage, fetcher)).status).toBe(404)
  status = 200
  expect((await cachedImage(request, storage, fetcher)).status).toBe(200)
  const failed = memoryCache(true)
  expect((await cachedImage(request, failed, fetcher)).status).toBe(200)
})
it("keeps more than 128 image URLs available offline", async () => {
  const storage = memoryCache()
  for (let i = 1; i <= 130; i += 1)
    await cachedImage(
      new Request(`https://cdn.test/images/card_l/${String(i)}.webp?v=1`),
      storage,
      () => Promise.resolve(new Response(String(i))),
    )
  const offline: typeof fetch = () => Promise.reject(new TypeError("offline"))
  expect(
    await (
      await cachedImage(new Request("https://cdn.test/images/card_l/1.webp?v=1"), storage, offline)
    ).text(),
  ).toBe("1")
  expect(await (await storage.open("sve-image-responses-v2")).keys()).toHaveLength(130)
})

it("removes older versions only for the same origin and pathname after a successful write", async () => {
  const storage = memoryCache()
  const base = "https://cdn.test/images/card_s/1.webp"
  const unchanged = [
    "https://preview.test/images/card_s/1.webp?v=1",
    "https://cdn.test/images/card_l/1.webp?v=1",
    "https://cdn.test/images/card_s/1-f2.webp?v=1",
    "https://cdn.test/other/images/card_s/1.webp?v=1",
  ]
  const network: typeof fetch = () => Promise.resolve(new Response("synthetic"))
  for (const url of [...unchanged, `${base}?v=9`, `${base}?v=10`])
    await cachedImage(new Request(url), storage, network)
  const cache = await storage.open("sve-image-responses-v2")
  expect((await cache.keys()).map((r) => r.url).sort()).toEqual(
    [...unchanged, `${base}?v=10`].sort(),
  )
})

it("a late older response cannot remove the newer cached version", async () => {
  const storage = memoryCache()
  const request = (v: number) => new Request(`https://cdn.test/images/card_l/1.webp?v=${String(v)}`)
  let release!: (response: Response) => void
  const pending = cachedImage(
    request(1),
    storage,
    () =>
      new Promise((resolve) => {
        release = resolve
      }),
  )
  await vi.waitFor(() => {
    expect(release).toBeDefined()
  })
  await cachedImage(request(2), storage, () => Promise.resolve(new Response("new")))
  release(new Response("old"))
  expect(await (await pending).text()).toBe("old")
  const offline: typeof fetch = () => Promise.reject(new Error("offline"))
  expect(await (await cachedImage(request(2), storage, offline)).text()).toBe("new")
})

it("preserves the previous image and returns the network response when the replacement write fails", async () => {
  const storage = memoryCache()
  const url = "https://cdn.test/images/card_l/1.webp"
  await cachedImage(new Request(`${url}?v=1`), storage, () => Promise.resolve(new Response("old")))
  const cache = await storage.open("sve-image-responses-v2")
  vi.spyOn(cache, "put").mockRejectedValue(new Error("quota"))
  vi.spyOn(storage, "open").mockResolvedValue(cache)
  expect(
    await (
      await cachedImage(new Request(`${url}?v=2`), storage, () =>
        Promise.resolve(new Response("new")),
      )
    ).text(),
  ).toBe("new")
  expect(await (await cache.match(`${url}?v=1`))?.text()).toBe("old")
})
it("intercepts only exact Bq GET URL shapes, keeping unversioned or duplicate query variants out", () => {
  expect(versionedImage("https://cdn.test/images/card_l/1-f7.webp?v=9")).toBe(true)
  for (const path of [
    "images/card_l/01.webp?v=2",
    "images/card_l/1-f0.webp?v=2",
    "images/card_l/1.webp",
    "images/card_l/1.webp?v=2&v=3",
    "images/card_l/1.webp?v=0",
    "images/sha256/abc.webp?v=2",
  ])
    expect(versionedImage(`https://cdn.test/${path}`)).toBe(false)
})
