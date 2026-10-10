// @vitest-environment node
import { describe, expect, it } from "vitest"

import { createAnnotatedTextResolver } from "../annotated-text"
import { createSnapshotClient } from "../client"
import { v3Fixture } from "../v3-fixture"
import { validateAnnotations } from "./annotations"
import {
  arrayValue,
  canonical,
  canonicalText,
  type JsonObject,
  objectValue,
  stringValue,
} from "./json"
import { expandNames, wholeName } from "./name-annotations"
import { readSnapshot, type View } from "./reader"
import { digest } from "./sha256"

const fixture = () => objectValue(v3Fixture("whole-name-native.json"))
function compactView(): View {
  const view = objectValue(fixture()["expected"]) as View
  view["annotation_set"] = []
  view["field_annotation"] = []
  for (const row of view["translation"] ?? []) row["annotation_set_id"] = null
  return view
}

function origin() {
  const data = fixture(),
    manifest = objectValue(data["manifest"]),
    files = new Map<string, Uint8Array>()
  for (const raw of arrayValue(manifest["files"])) {
    const file = objectValue(raw)
    files.set(
      stringValue(file["path"]),
      canonical(objectValue(data["payloads"])[stringValue(file["key"])] ?? null),
    )
  }
  const bytes = canonical(manifest),
    hash = digest(bytes),
    path = `snapshots/manifests/${hash.slice(7)}.json`
  files.set(path, bytes)
  files.set(
    "snapshots/preview/current.json",
    canonical({ manifest_path: path, manifest_sha256: hash }),
  )
  const client = createSnapshotClient("/cdn", {
    entry: "preview",
    fetch: (url) => {
      const raw = files.get(url.slice("/cdn/".length))
      return Promise.resolve(
        raw ? new Response(raw.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  return { data, client }
}

describe("whole names on exact source owners", () => {
  it("restores the native producer's logical occurrences and permanent IDs", () => {
    const data = fixture()
    const payloads = new Map(
      Object.entries(objectValue(data["payloads"])).map(([key, value]) => [key, canonical(value)]),
    )
    expect(canonicalText(readSnapshot(data["manifest"] ?? null, payloads))).toBe(
      canonicalText(data["expected"] ?? null),
    )
    const view = compactView()
    expandNames(view)
    expect(view["annotation_set"]).toHaveLength(2)
    expect(view["field_annotation"]).toHaveLength(1)
    expect(view["translation"]?.[0]?.["annotation_set_id"]).toBe(
      objectValue(data["expected"])["translation"] &&
        (objectValue(data["expected"])["translation"] as JsonObject[])[0]?.["annotation_set_id"],
    )
  })
  it("uses scalar length for an exact unnormalized whole name", () => {
    const unit = { id: "t:ja:synthetic", text: "A😀Ｂé手牌" }
    expect(wholeName(unit, "term:whole")["occurrences"]).toEqual([
      {
        ordinal: 0,
        reference: { kind: "card_name", term_id: "term:whole" },
        ranges: [{ start: 0, end: 7 }],
        bold: true,
      },
    ])
  })
  it.each(["effect", "source", "concept", "receiver", "unused"])(
    "rejects %s confusion",
    (mutation) => {
      const view = compactView()
      const receiver = view["face_revision"]?.[0]
      if (!receiver) throw new Error("missing test receiver")
      const selection = objectValue(arrayValue(receiver["translations"])[0])
      if (mutation === "effect") {
        selection["field"] = "effect"
        objectValue(selection["source"])["field"] = "effect"
      }
      if (mutation === "source")
        objectValue(objectValue(selection["source"])["owner"])["id"] = "missing"
      if (mutation === "concept") receiver["name_concept_id"] = null
      if (mutation === "receiver") {
        const donor = {
          ...receiver,
          id: "historical",
          name_concept_id: "different",
          translations: [],
        }
        view["face_revision"]?.push(donor)
        objectValue(objectValue(selection["source"])["owner"])["id"] = "historical"
      }
      if (mutation === "unused") receiver["translations"] = []
      expect(() => {
        validateAnnotations(view, ["ja", "zh-Hant"])
      }).toThrow()
    },
  )
  it("makes the partial annotated view ready after resolving its source and concepts", async () => {
    const { data, client } = origin()
    await client.load()
    expect(client.status().state).toBe("ready")
    const view = await createAnnotatedTextResolver(client).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(view?.original.annotation?.["id"]).toBe(
      (objectValue(data["expected"])["field_annotation"] as JsonObject[])[0]?.["annotation_set_id"],
    )
    expect(view?.translated?.annotation?.["id"]).toBe(
      (objectValue(data["expected"])["translation"] as JsonObject[])[0]?.["annotation_set_id"],
    )
    expect(view?.translation?.["annotation_kind"]).toBe("whole_name")
  })
  it("stays pending while the exact source is fetching, then rejects a complete missing source", async () => {
    const { client } = origin()
    await client.load()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("missing loaded snapshot")
    const revision = snapshot.bootstrap.find((fragment) => fragment.table === "face_revision")
      ?.rows[0]
    if (!revision) throw new Error("missing test revision")
    const selection = objectValue(arrayValue(revision["translations"])[0])
    objectValue(objectValue(selection["source"])["owner"])["id"] = "missing-exact-source"
    const gate = Promise.withResolvers<undefined>()
    const entered = Promise.withResolvers<undefined>()
    const resolver = createAnnotatedTextResolver({
      ...client,
      fragments: async (file) => {
        entered.resolve(undefined)
        await gate.promise
        return client.fragments(file)
      },
    })
    let settled = false
    const pending = resolver.resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    const rejection = expect(pending).rejects.toThrow()
    void pending.then(
      () => {
        settled = true
      },
      () => {
        settled = true
      },
    )
    await entered.promise
    expect(settled).toBe(false)
    expect(revision["name_unit_id"]).toBeTruthy()
    gate.resolve(undefined)
    await rejection
    expect(settled).toBe(true)
  })
})
