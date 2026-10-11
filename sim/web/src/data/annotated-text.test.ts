// @vitest-environment node
import { describe, expect, it } from "vitest"

import { annotatedOrigin } from "../test-utils/annotated-snapshot"
import { createAnnotatedTextResolver, sharedAnnotatedTextResolver } from "./annotated-text"

describe("the native C → P → R annotation slice", () => {
  it("validates bootstrap name annotations and their explanation without loading unrelated files", async () => {
    const { client, requests } = await annotatedOrigin()
    await client.load()
    expect(client.status().state).toBe("ready")
    const before = requests.length
    const value = await createAnnotatedTextResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(value?.original.annotation?.["occurrences"]).toHaveLength(1)
    expect(value?.translated?.annotation?.["occurrences"]).toHaveLength(1)
    expect(value?.translated?.unit["text"]).toBe("Synthetic translation")
    expect(value?.selection?.["basis"]).toBe("own_source")
    expect(value?.translation?.["low_confidence"]).toBe(false)
    expect(value?.explanations[0]?.reference).toEqual({ kind: "keyword", id: "keyword" })
    expect(requests).toHaveLength(before)
  })
  it("uses the exact effect owner and treats verified absent annotations as empty", async () => {
    const { client, requests } = await annotatedOrigin()
    await client.load()
    const before = requests.length
    const value = await createAnnotatedTextResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "effect", ordinal: null },
      "en",
    )
    expect(value?.original.annotation).toBeNull()
    expect(value?.translated).toBeUndefined()
    expect(requests.length).toBeGreaterThan(before)
    expect(requests.some((path) => path.includes("text_all"))).toBe(false)
  })
  it("shares one resolver per loaded snapshot and resolves each text once", async () => {
    const { client, requests } = await annotatedOrigin()
    await client.load()
    const resolver = sharedAnnotatedTextResolver(client)
    expect(sharedAnnotatedTextResolver(client)).toBe(resolver)
    const effect = {
      owner: { kind: "face_revision", id: "revision" },
      field: "effect",
      ordinal: null,
    }
    const name = { ...effect, field: "name" }
    expect(resolver.peek(effect, "en")).toBeUndefined()
    // Texts resolved together load their shared files once and still pass validation.
    const [first, again, other] = await Promise.all([
      resolver.resolve(effect, "en"),
      resolver.resolve(effect, "en"),
      resolver.resolve(name, "zh-Hant"),
    ])
    expect(again).toBe(first)
    expect(other?.translated?.unit["text"]).toBe("Synthetic translation")
    expect(resolver.peek(effect, "en")).toBe(first)
    const settled = requests.length
    expect(await resolver.resolve(effect, "en")).toBe(first)
    expect(requests).toHaveLength(settled)
  })
  it("does not expose settled text after the client's snapshot is replaced", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    let current = client.snapshot()
    const resolver = createAnnotatedTextResolver({ ...client, snapshot: () => current })
    const pointer = {
      owner: { kind: "face_revision", id: "revision" },
      field: "name",
      ordinal: null,
    }
    expect(await resolver.resolve(pointer, "en")).not.toBeNull()
    current = null
    expect(resolver.peek(pointer, "en")).toBeUndefined()
    await expect(resolver.resolve(pointer, "en")).rejects.toThrow("snapshot replaced")
  })
  it("rejects text finishing after the client's snapshot is replaced", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    let current = client.snapshot()
    const resolver = createAnnotatedTextResolver({ ...client, snapshot: () => current })
    const pointer = {
      owner: { kind: "face_revision", id: "revision" },
      field: "name",
      ordinal: null,
    }
    const pending = resolver.resolve(pointer, "en")
    current = null
    await expect(pending).rejects.toThrow("snapshot replaced")
    expect(resolver.peek(pointer, "en")).toBeUndefined()
  })
  it("rejects an absent owner rather than borrowing annotations of the same text", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    await expect(
      createAnnotatedTextResolver(client).resolve(
        { owner: { kind: "face_revision", id: "other" }, field: "name", ordinal: null },
        "zh-Hant",
      ),
    ).rejects.toThrow()
  })
  it.each(["name", "effect", "flavor"])(
    "keeps an unknown printed %s separate from valid current text",
    async (field) => {
      const { client } = await annotatedOrigin()
      await client.load()
      const resolver = createAnnotatedTextResolver(client)
      expect(
        await resolver.resolve(
          {
            owner: { kind: "printing_face", id: "printing", face_id: "face" },
            field,
            ordinal: null,
          },
          "zh-Hant",
        ),
      ).toBeNull()
    },
  )
})
