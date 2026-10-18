// @vitest-environment node
import { describe, expect, it } from "vitest"

import { annotatedOrigin } from "../test-utils/annotated-snapshot"
import { createAnnotatedTextResolver, sharedAnnotatedTextResolver } from "./annotated-text"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  objectValue,
  stringValue,
} from "./format-v3/json"

async function historicalCandidateOrigin() {
  const { client } = await annotatedOrigin()
  await client.load()
  const loaded = client.snapshot()
  if (!loaded) throw new Error("synthetic snapshot missing")
  const detail = await client.fragments("detail/jp/family-family/0")
  const revision = detail.find((fragment) => fragment.table === "face_revision")
  if (!revision) throw new Error("synthetic revision missing")
  const historyKey = "history/jp/family-family/0"
  const historyValue: JsonObject = {
    ...revision.value,
    base: null,
    partition: "history",
    purpose: "history",
    region: "jp",
    segment: "family-family:band-0",
  }
  const history = {
    ...revision,
    file: historyKey,
    value: historyValue,
    rows: revision.rows.map((row) => ({ ...row, id: "historical" })),
    identity: canonicalText([
      "face_revision",
      historyValue["owner"] ?? null,
      historyValue["segment"] ?? null,
      "history",
    ]),
  }
  const files = new Map(loaded.files)
  files.set(historyKey, {
    key: historyKey,
    role: "text",
    purpose: "history",
    region: "jp",
    segment: "family-family:band-0",
    row_counts: [
      [
        "face_revision",
        "home_set",
        "family",
        "history",
        "family-family:band-0",
        revision.rows.length,
      ],
    ],
  })
  const snapshot = {
    ...loaded,
    files,
    bootstrap: loaded.bootstrap.map((fragment) =>
      fragment.table === "face"
        ? {
            ...fragment,
            rows: fragment.rows.map((row) => ({
              ...row,
              wording: [
                {
                  region: "jp",
                  state: "pending",
                  display: { revision_id: "revision", basis: "candidates" },
                  candidates: [{ printing_id: "printing", revision_id: "historical" }],
                  undated_printing_ids: [],
                },
              ],
            })),
          }
        : fragment,
    ),
  }
  const requested: string[] = []
  return {
    requested,
    client: {
      ...client,
      snapshot: () => snapshot,
      fragments: async (key: string) => {
        requested.push(key)
        return key === historyKey ? [history] : client.fragments(key)
      },
    },
    revision: objectValue(revision.rows[0]),
    historyKey,
  }
}

describe("the native C → P → R annotation slice", () => {
  it("discovers and resolves a candidate's source sections with their original ordinals", async () => {
    const { client, historyKey, revision } = await historicalCandidateOrigin()
    const unitId = stringValue(revision["effect_unit_id"])
    const source = {
      ...client,
      fragments: async (key: string) => {
        const fragments = await client.fragments(key)
        return key === historyKey
          ? fragments.map((fragment) => ({
              ...fragment,
              rows: fragment.rows.map((row) => ({
                ...row,
                sections: [1, 3].map((ordinal) => ({
                  ordinal,
                  label_unit_id: ordinal === 3 ? unitId : null,
                  text_unit_id: unitId,
                })),
              })),
            }))
          : fragments
      },
    }
    const resolver = createAnnotatedTextResolver(source)
    const owner = { kind: "face_revision", id: "historical" }
    const effect = await resolver.resolve({ owner, field: "effect", ordinal: null }, "ja")
    expect(effect?.originalSections).toEqual([
      { ordinal: 1 },
      { ordinal: 3, label: "Synthetic text" },
    ])
    for (const { ordinal } of effect?.originalSections ?? []) {
      const section = await resolver.resolve({ owner, field: "section", ordinal }, "ja")
      expect(section?.original.unit["text"]).toBe("Synthetic text")
      expect(section?.translated).toBeUndefined()
    }
  })
  it("locates a direct historical candidate through its face without a translation receiver", async () => {
    const { client, requested, revision, historyKey } = await historicalCandidateOrigin()
    const resolver = createAnnotatedTextResolver(client)
    const pointer = {
      owner: { kind: "face_revision", id: "historical" },
      field: "effect",
      ordinal: null,
    }
    const value = await resolver.resolve(pointer, "ja")
    expect(value?.original.unit["id"]).toBe(revision["effect_unit_id"])
    expect(value?.original.unit["text"]).toBe("Synthetic text")
    expect(requested).toContain(historyKey)
    expect(requested.filter((key) => key === historyKey)).toHaveLength(1)
    const before = requested.length
    expect(await resolver.resolve(pointer, "ja")).toBe(value)
    expect(requested).toHaveLength(before)
    await expect(
      resolver.resolve({ ...pointer, owner: { kind: "face_revision", id: "unlisted" } }, "ja"),
    ).rejects.toThrow("owner")
    expect(requested).toHaveLength(before)
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
  it("projects a confirmed card-name target with the fields its link and label need", async () => {
    const { client } = await annotatedOrigin()
    await client.load()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("synthetic snapshot missing")
    // The producer confirms the concept-to-card relation; the reader only projects it.
    for (const fragment of snapshot.bootstrap) {
      if (fragment.table === "annotation_concept")
        for (const row of fragment.rows)
          if (row["id"] === "term") {
            row["category"] = "card_name"
            row["card_ids"] = ["card"]
          }
      if (fragment.table === "annotation_set")
        for (const row of fragment.rows)
          for (const raw of arrayValue(row["occurrences"]))
            objectValue(raw)["reference"] = { kind: "card_name", term_id: "term" }
    }
    const value = await createAnnotatedTextResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(value?.concepts.map((concept) => concept["card_ids"])).toEqual([["card"]])
    expect(
      value?.cards.map((card) => ({
        cardId: card.cardId,
        intId: card.intId,
        cardNo: card.cardNo,
        cardNoState: card.cardNoState,
        region: card.region,
        name: card.name.original.text,
      })),
    ).toEqual([
      {
        cardId: "card",
        intId: 20001,
        cardNo: "TEST-001Ⓢa",
        cardNoState: "official",
        region: "jp",
        name: stringValue(value?.original.unit["text"]),
      },
    ])
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
