import { afterEach, beforeEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { catalogOf, createSnapshotClient, peekAnnotatedTextResolver, stringValue } from "../data"
import { memoryCache } from "../test-utils/cache"
import type { CardContent } from "./card-content"
import { prefetchCards } from "./card-prefetch"

const decode = vi.fn<(this: HTMLImageElement) => Promise<void>>()

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })

async function setup() {
  const client = createSnapshotClient("https://prefetch.test", {
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const bytes = built.files.get(url.slice("https://prefetch.test/".length))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await client.load()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing synthetic snapshot")
  const catalog = catalogOf(snapshot)
  const targets = ["p:bp01-001", "p:bp01-002", "p:bp01-003"].map((id) => {
    const summary = catalog.summary(id)
    if (!summary) throw new Error("missing synthetic printing")
    return { summary, neighbour: true }
  })
  return { client, targets }
}

beforeEach(() => {
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    value: decode,
  })
  decode.mockReset().mockResolvedValue(undefined)
})
afterEach(() => vi.restoreAllMocks())

it("limits shared work to two cards and cancels queued and late results", async () => {
  const { client, targets } = await setup()
  const images: HTMLImageElement[] = []
  const releases: (() => void)[] = []
  decode.mockImplementation(function (this: HTMLImageElement) {
    images.push(this)
    return new Promise<void>((resolve) => releases.push(resolve))
  })
  const first = new AbortController()
  const second = new AbortController()
  const accepted: CardContent[] = []
  const one = prefetchCards(client, targets.slice(0, 2), "zh-Hant", first.signal, (_, value) =>
    accepted.push(value),
  )
  const two = prefetchCards(client, targets.slice(2), "zh-Hant", second.signal, (_, value) =>
    accepted.push(value),
  )
  await vi.waitFor(() => {
    expect(images).toHaveLength(2)
  })
  expect(images.every((image) => image.fetchPriority === "low")).toBe(true)
  second.abort()
  first.abort()
  await Promise.all([one, two])
  for (const release of releases) release()
  await Promise.resolve()
  expect(images).toHaveLength(2)
  expect(images.every((image) => !image.hasAttribute("src"))).toBe(true)
  expect(accepted).toHaveLength(0)
})

it("prepares route, basic wording and images without selecting another active region", async () => {
  const { client, targets } = await setup()
  const snapshot = await client.catalog?.("en", new AbortController().signal)
  if (!snapshot) throw new Error("missing English catalog")
  const jp = targets[0]
  if (!jp) throw new Error("missing Japanese printing")
  const catalog = catalogOf(snapshot)
  const other = catalog.index.printingsOf(jp.summary.cardId).find((row) => row["region"] === "en")
  const english = other ? catalog.summary(stringValue(other["id"])) : undefined
  if (!english) throw new Error("missing related English printing")
  const accepted: CardContent[] = []
  await prefetchCards(
    client,
    [jp, { summary: english, neighbour: false }],
    "zh-Hant",
    new AbortController().signal,
    (_, value) => accepted.push(value),
  )
  expect(accepted.map((value) => value.printing.region).sort()).toEqual(["en", "jp"])
  for (const value of accepted) {
    const face = value.catalog.index.facesOf(value.printing.cardId)[0]
    if (!face) throw new Error("missing face")
    expect(value.images.cardImage(value.printing.id, stringValue(face["id"]))).toBeDefined()
    expect(
      peekAnnotatedTextResolver(value.client)?.peek(
        {
          owner: { kind: "printing_face", id: value.printing.id, face_id: stringValue(face["id"]) },
          field: "effect",
          ordinal: null,
        },
        "zh-Hant",
      ),
    ).not.toBeUndefined()
  }
  expect(client.snapshot()?.region).toBe("jp")
})

it("ignores a failed prefetch and prepares the remaining card", async () => {
  const { client, targets } = await setup()
  decode.mockRejectedValueOnce(new Error("synthetic image failure"))
  const accepted: CardContent[] = []
  await prefetchCards(
    client,
    targets.slice(0, 2),
    "zh-Hant",
    new AbortController().signal,
    (_, value) => accepted.push(value),
  )
  expect(accepted).toHaveLength(1)
  expect(client.status().state).toBe("ready")
})
