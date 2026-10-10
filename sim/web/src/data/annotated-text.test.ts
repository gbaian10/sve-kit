// @vitest-environment node
import { describe, expect, it } from "vitest"

import { annotatedOrigin } from "../test-utils/annotated-snapshot"
import { createAnnotatedTextResolver } from "./annotated-text"
import { canonicalText } from "./format-v3/json"
import { readSnapshot } from "./format-v3/reader"

describe("the native C → P → R annotation slice", () => {
  it("reads the same nonempty snapshot produced from stored semantic positions", async () => {
    const { fixture, manifest, payloads } = await annotatedOrigin()
    expect(canonicalText(readSnapshot(manifest, payloads))).toBe(
      canonicalText(fixture["expected"] ?? null),
    )
  })
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
