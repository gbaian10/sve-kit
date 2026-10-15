// @vitest-environment node
import { describe, expect, it } from "vitest"

import fixture from "../../../../tests/fixtures/snapshot-contract/v3/source-native.json"
import { annotatedOrigin } from "../test-utils/annotated-snapshot"
import { createAnnotatedTextResolver } from "./annotated-text"
import { arrayValue, type JsonObject, objectValue, stringValue } from "./format-v3/json"

async function origin(region: "jp" | "en" = "en", input: JsonObject = fixture) {
  const value = await annotatedOrigin(input, region)
  await value.client.load()
  expect(value.client.status().state).toBe("ready")
  return value
}

describe("one-hop source presentation", () => {
  it("loads the exact Japanese owner, body, sections and separate annotations on demand", async () => {
    const { client, requests, manifest } = await origin()
    const files = arrayValue(manifest["files"]).map((raw) => objectValue(raw))
    expect(
      requests.some((path) =>
        files.some((file) => file["region"] === "jp" && file["path"] === path),
      ),
    ).toBe(false)
    const before = requests.length
    const result = await createAnnotatedTextResolver(client).resolve(fixture.receiver, "zh-Hant")
    expect(result?.original.unit["text"]).toBe("Synthetic English counterpart effect")
    expect(result?.source?.unit["text"]).toBe("Synthetic Japanese source effect")
    expect(result?.translated?.unit["text"]).toBe("Synthetic translated source effect")
    expect(result?.sourceSections?.map((s) => s.unit["text"])).toEqual([
      "Synthetic Japanese source paragraph",
    ])
    const texts = [
      result?.original,
      result?.source,
      result?.translated,
      ...(result?.sourceSections ?? []),
    ]
    for (const text of texts) {
      expect(text?.annotation?.["text_unit_id"]).toBe(text?.unit["id"])
      expect(text?.annotation?.["occurrences"]).toHaveLength(1)
    }
    const added = new Set(
      requests
        .slice(before)
        .map((path) => stringValue(files.find((f) => f["path"] === path)?.["key"])),
    )
    expect(added).toEqual(
      new Set([
        "detail/en/family-BP02/0",
        "translation/shared/family-BP02/0",
        "catalog/jp/family-BP02/0",
        "detail/jp/family-BP02/0",
      ]),
    )
  })

  it("resolves an official counterpart with the counterpart's own annotation set", async () => {
    const { client } = await origin("jp")
    const result = await createAnnotatedTextResolver(client).resolve(fixture.counterpart, "en")
    expect(result?.selection?.["basis"]).toBe("official_counterpart")
    expect(result?.translated?.unit["text"]).toBe("Synthetic English counterpart effect")
    expect(result?.counterpart).toEqual(result?.translated)
    expect(result?.original.annotation?.["id"]).not.toBe(result?.counterpart?.annotation?.["id"])
  })

  it.each(["bad_hash", "missing_source_file", "shared_translation_region"])(
    "rejects %s at manifest admission",
    async (mutation) => {
      const input: JsonObject = structuredClone(fixture)
      const manifest = objectValue(input["manifest"])
      const files = arrayValue(manifest["files"])
      const file = files
        .map((raw) => objectValue(raw))
        .find((f) => f["key"] === "detail/en/family-BP02/0")
      if (!file) throw new Error("missing synthetic descriptor")
      const ref = objectValue(arrayValue(file["source_refs"])[0])
      if (mutation === "bad_hash") ref["sha256"] = `sha256:${"0".repeat(64)}`
      else if (mutation === "missing_source_file")
        manifest["files"] = files.filter((raw) => objectValue(raw)["key"] !== ref["key"])
      else {
        const common = files
          .map((raw) => objectValue(raw))
          .find((f) => f["purpose"] === "translation")
        if (!common) throw new Error("missing shared translation")
        common["region"] = "en"
      }
      const { client } = await annotatedOrigin(input, "en")
      await client.load()
      expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
      expect(client.snapshot()).toBeNull()
    },
  )

  it("rejects a source whose manifest scope disagrees with its decoded owner", async () => {
    const input: JsonObject = structuredClone(fixture)
    const file = arrayValue(objectValue(input["manifest"])["files"])
      .map((raw) => objectValue(raw))
      .find((f) => f["key"] === "detail/jp/family-BP02/0")
    if (!file) throw new Error("missing synthetic descriptor")
    file["region"] = "en"
    const { client } = await origin("en", input)
    const read = async () =>
      createAnnotatedTextResolver(client).resolve(fixture.receiver, "zh-Hant")
    await expect(read()).rejects.toThrow(/region|scope|coordinate/)
  })

  it.each(["missing_source", "bad_pointer", "missing_dictionary", "conflicting_copy"])(
    "rejects %s without borrowing another owner's text",
    async (mutation) => {
      const input: JsonObject = structuredClone(fixture)
      const { client } = await origin("en", input)
      const snapshot = client.snapshot()
      if (!snapshot) throw new Error("missing synthetic snapshot")
      const target = "detail/en/family-BP02/0"
      const corrupt = {
        ...client,
        fragments: async (key: string) => {
          const fragments = structuredClone(await client.fragments(key))
          if (key === target)
            for (const fragment of fragments) {
              if (mutation === "missing_dictionary" && fragment.table === "text_unit")
                fragment.rows.length = 0
              if (mutation === "bad_pointer" && fragment.table === "face_revision") {
                const selection = objectValue(arrayValue(fragment.rows[0]?.["translations"])[0])
                objectValue(objectValue(selection["source"])["owner"])["id"] = "missing-owner"
              }
              if (mutation === "conflicting_copy" && fragment.table === "text_unit") {
                // The bootstrap's existing name is an independently loaded same-ID copy.
                const name = snapshot.bootstrap.find((f) => f.table === "text_unit")?.rows[0]
                if (!name) throw new Error("missing name")
                fragment.rows.push({ ...name, text: "Conflicting synthetic copy" })
              }
            }
          return fragments
        },
      }
      if (mutation === "missing_source") {
        const file = snapshot.files.get(target)
        if (!file) throw new Error("missing source descriptor")
        file["source_refs"] = []
      }
      await expect(
        createAnnotatedTextResolver(corrupt).resolve(fixture.receiver, "zh-Hant"),
      ).rejects.toThrow(/owner|reference/)
    },
  )
})
