// @vitest-environment node
import { describe, expect, it } from "vitest"

import { createAnnotatedTextResolver } from "../annotated-text"
import { createSnapshotClient, type SnapshotClient } from "../client"
import { bucketOf, homeSetOwner } from "../locator"
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
import { type Fragment, readSnapshot, type View } from "./reader"
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

async function sourceHistory() {
  const { client } = origin()
  await client.load()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing loaded snapshot")
  const revision = snapshot.bootstrap.find((fragment) => fragment.table === "face_revision")
    ?.rows[0]
  const card = snapshot.bootstrap
    .find((fragment) => fragment.table === "card")
    ?.rows.find((row) => row["id"] === "card")
  if (!revision || !card) throw new Error("missing test receiver")
  const donor = {
    ...(objectValue(fixture()["expected"])["face_revision"] as JsonObject[])[0],
    id: "historical-jp-source",
    translations: [],
  }
  revision["region"] = "en"
  revision["name_concept_id"] = null
  for (const raw of arrayValue(card["regions"])) objectValue(raw)["mapping_state"] = "confirmed"
  const selection = objectValue(arrayValue(revision["translations"])[0])
  selection["basis"] = "jp_source"
  objectValue(objectValue(selection["source"])["owner"])["id"] = donor.id
  const owner = homeSetOwner(stringValue(card["home_set_id"]))
  const bucket = bucketOf([card["id"] ?? null], 64)
  const historyKey = `text/history/home_set/family/band/${String(Math.floor(bucket / 32))}`
  snapshot.files.set(historyKey, {
    role: "text",
    row_counts: [{ table: "face_revision", partition: "history", owner, bucket, count: 1 }],
  })
  // These manifest candidates must never be fetched for this card's source closure.
  for (let index = 0; index < 20; index++)
    snapshot.files.set(`unrelated-${String(index)}`, {
      role: "text",
      row_counts: [
        {
          table: "face_revision",
          partition: "history",
          owner: homeSetOwner(`other-${String(index)}`),
          bucket,
          count: 1,
        },
      ],
    })
  const fragment: Fragment = {
    file: historyKey,
    table: "face_revision",
    identity: canonicalText(["face_revision", owner, bucket, "history"]),
    value: { owner, bucket, partition: "history", base: null },
    rows: [donor],
  }
  return { client, snapshot, revision, historyKey, fragment }
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
    const { client, revision, historyKey } = await sourceHistory()
    const requests: string[] = []
    const gate = Promise.withResolvers<undefined>()
    const entered = Promise.withResolvers<undefined>()
    const resolver = createAnnotatedTextResolver({
      ...client,
      fragments: async (file) => {
        requests.push(file)
        if (file !== historyKey) throw new Error("unrelated source candidate fetched")
        entered.resolve(undefined)
        await gate.promise
        return []
      },
    })
    let settled = false
    const pending = resolver.resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    const rejection = expect(pending).rejects.toThrow("public-annotation/owner")
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
    expect(requests).toEqual([historyKey])
  })
  it("loads only the receiver card's history partition for an exact JP donor", async () => {
    const { client, historyKey, fragment } = await sourceHistory()
    const requests: string[] = []
    const scopedClient: SnapshotClient = {
      ...client,
      fragments: (file) => {
        requests.push(file)
        if (file !== historyKey) throw new Error("unrelated source candidate fetched")
        return Promise.resolve([fragment])
      },
    }
    const result = await createAnnotatedTextResolver(scopedClient).resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(result?.selection?.["basis"]).toBe("jp_source")
    expect(result?.source?.annotation?.["occurrences"]).toHaveLength(1)
    expect(result?.translated?.annotation?.["occurrences"]).toHaveLength(1)
    expect(requests).toEqual([historyKey])
  })
  it("reports a missing source directly when its card has no candidate partition", async () => {
    const { client, snapshot, historyKey } = await sourceHistory()
    snapshot.files.delete(historyKey)
    const resolver = createAnnotatedTextResolver({
      ...client,
      fragments: () => {
        throw new Error("unrelated source candidate fetched")
      },
    })
    await expect(
      resolver.resolve(
        { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
        "zh-Hant",
      ),
    ).rejects.toThrow("public-annotation/owner")
  })
  it("uses restored name sets without fetching their unrelated detail bucket", async () => {
    const { data, client } = origin()
    await client.load()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("missing loaded snapshot")
    const ids = (objectValue(data["expected"])["annotation_set"] as JsonObject[]).map(
      (row) => row["id"] ?? null,
    )
    snapshot.files.set("virtual-set-trap", {
      role: "text",
      row_counts: ids.map((id) => ({
        table: "annotation_set",
        owner: { kind: "global", id: null },
        bucket: bucketOf([id], 64),
        partition: "detail",
        count: 1,
      })),
    })
    const resolver = createAnnotatedTextResolver({
      ...client,
      fragments: () => {
        throw new Error("virtual set detail bucket fetched")
      },
    })
    const result = await resolver.resolve(
      { owner: { kind: "face_revision", id: "revision" }, field: "name", ordinal: null },
      "zh-Hant",
    )
    expect(result?.original.annotation?.["id"]).toBe(
      (objectValue(data["expected"])["field_annotation"] as JsonObject[])[0]?.["annotation_set_id"],
    )
    expect(result?.translated?.annotation).toBeTruthy()
  })
})
