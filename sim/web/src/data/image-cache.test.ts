// @vitest-environment node
import { expect, it } from "vitest"

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
  const request = new Request("https://cdn.test/images/card_m/1.webp?v=2")
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
  const request = new Request("https://cdn.test/images/card_m/1.webp?v=2")
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
    new Request(`https://cdn.test/images/card_m/20001.webp?v=${String(v)}`)
  expect(await (await cachedImage(request(1), storage, network)).text()).toBe("A")
  bytes = "B"
  expect(await (await cachedImage(request(2), storage, network)).text()).toBe("B")
  expect(await (await cachedImage(request(1), storage, network)).text()).toBe("A")
  expect(urls).toHaveLength(2)
  failed = true
  await expect(cachedImage(request(3), storage, network)).rejects.toThrow("offline")
  expect(await (await cachedImage(request(2), storage, network)).text()).toBe("B")
})
it("does not cache negative responses, and tolerates storage failure without an old-image fallback", async () => {
  const storage = memoryCache()
  let status = 404
  const request = new Request("https://cdn.test/images/card_m/1.webp?v=2")
  const fetcher: typeof fetch = () => Promise.resolve(new Response("synthetic", { status }))
  expect((await cachedImage(request, storage, fetcher)).status).toBe(404)
  status = 200
  expect((await cachedImage(request, storage, fetcher)).status).toBe(200)
  const failed = memoryCache(true)
  expect((await cachedImage(request, failed, fetcher)).status).toBe(200)
})
it("bounds the response cache across versions and keeps roots apart", async () => {
  const storage = memoryCache()
  for (let i = 1; i <= 130; i += 1)
    await cachedImage(
      new Request(`https://cdn.test/images/card_m/1.webp?v=${String(i)}`),
      storage,
      () => Promise.resolve(new Response(String(i))),
    )
  const cache = await storage.open("sve-image-responses-v2")
  const keys = await cache.keys()
  expect(keys).toHaveLength(128)
  expect(keys.some((r) => r.url.endsWith("?v=1"))).toBe(false)
  let read = false
  expect(
    await (
      await cachedImage(
        new Request("https://preview.test/images/card_m/1.webp?v=130"),
        storage,
        () => {
          read = true
          return Promise.resolve(new Response("preview"))
        },
      )
    ).text(),
  ).toBe("preview")
  expect(read).toBe(true)
})
it("intercepts only exact Bq GET URL shapes, keeping unversioned or duplicate query variants out", () => {
  expect(versionedImage("https://cdn.test/images/card_l/1-f7.webp?v=9")).toBe(true)
  for (const path of [
    "images/card_m/01.webp?v=2",
    "images/card_m/1-f0.webp?v=2",
    "images/card_m/1.webp",
    "images/card_m/1.webp?v=2&v=3",
    "images/card_m/1.webp?v=0",
    "images/sha256/abc.webp?v=2",
  ])
    expect(versionedImage(`https://cdn.test/${path}`)).toBe(false)
})
