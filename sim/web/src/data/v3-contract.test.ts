// @vitest-environment node
import { describe, expect, it } from "vitest"

import { createSnapshotClient } from "./client"
import { type ReaderErrorCode, SnapshotError } from "./format-v3/errors"
import {
  arrayValue,
  canonical,
  canonicalText,
  isObject,
  type JsonObject,
  type JsonPath,
  type JsonValue,
  objectValue,
  parseStrict,
  stringValue,
  utf8,
} from "./format-v3/json"
import { readSnapshot, readTextAll } from "./format-v3/reader"
import { validate } from "./format-v3/schema"
import { digest } from "./format-v3/sha256"
import { imageUrl } from "./image-url"
import { v3Fixture } from "./v3-fixture"

const inventory = objectValue(v3Fixture("index.json"))
const cases = (field: string): JsonObject[] =>
  arrayValue(v3Fixture(stringValue(inventory[field]))).map((v) => objectValue(v))

function wire() {
  const manifest = objectValue(v3Fixture(stringValue(inventory["manifest"])))
  const files = arrayValue(manifest["files"]).map((f) => objectValue(f))
  const payloads = new Map(
    files.map((f) => [
      stringValue(f["key"]),
      objectValue(v3Fixture(`payloads/${stringValue(f["sha256"]).slice(7)}.json`)),
    ]),
  )
  return { manifest, files, payloads }
}

function replace(value: JsonValue, path: JsonPath, replacement: JsonValue): void {
  let current = value as JsonObject | JsonValue[]
  for (const part of path.slice(0, -1))
    current = (current as Record<string, JsonValue>)[String(part)] as JsonObject | JsonValue[]
  ;(current as Record<string, JsonValue>)[String(path.at(-1))] = replacement
}

// Reseal the same dependency order as the shared mutation protocol, without repairing membership.
function reseal(input: ReturnType<typeof wire>): Map<string, Uint8Array> {
  const byKey = new Map(input.files.map((f) => [stringValue(f["key"]), f]))
  function rebind(value: JsonValue): void {
    if (Array.isArray(value)) value.forEach(rebind)
    else if (isObject(value)) {
      if (Object.keys(value).sort().join(",") === "key,sha256") {
        const file = byKey.get(stringValue(value["key"]))
        if (file) value["sha256"] = file["sha256"] ?? null
      } else Object.values(value).forEach(rebind)
    }
  }
  const rank = (f: JsonObject): number =>
    ["config", "programs", "bootstrap"].includes(stringValue(f["role"]))
      ? 0
      : f["role"] === "images"
        ? 2
        : 1
  const result = new Map<string, Uint8Array>()
  for (const file of [...input.files].sort(
    (a, b) => rank(a) - rank(b) || stringValue(a["key"]).localeCompare(stringValue(b["key"])),
  )) {
    const key = stringValue(file["key"])
    const value = input.payloads.get(key)
    if (!value) throw new Error("missing golden payload")
    rebind(value)
    rebind(file["dependencies"] ?? null)
    const bytes = canonical(value)
    file["sha256"] = digest(bytes)
    file["bytes"] = bytes.length
    file["path"] = `snapshots/blobs/${digest(bytes).slice(7)}.json`
    file["row_counts"] = Object.entries(objectValue(value["tables"] ?? {})).flatMap(
      ([table, fragments]) =>
        arrayValue(fragments).map((raw) => {
          const fragment = objectValue(raw)
          return {
            table,
            owner: fragment["owner"] ?? null,
            bucket: fragment["bucket"] ?? null,
            partition: fragment["partition"] ?? null,
            count: arrayValue(fragment["rows"]).length,
          }
        }),
    )
    result.set(key, bytes)
  }
  rebind(input.manifest["config_ref"] ?? null)
  rebind(input.manifest["text_all"] ?? null)
  return result
}

function mutatedWire(item: JsonObject) {
  const input = wire()
  for (const mutation of [...arrayValue(item["setup"] ?? []), item].map((v) => objectValue(v))) {
    const target = stringValue(mutation["target"])
    const value = target === "manifest" ? input.manifest : input.payloads.get(target)
    if (!value) throw new Error("missing mutation target")
    replace(value, arrayValue(mutation["path"]) as JsonPath, mutation["value"] ?? null)
  }
  return input
}

const expectedCodes: Record<string, ReaderErrorCode> = {
  "available-missing-size": "image-variant-unapproved",
  "available-reverse-size": "image-variant-unapproved",
  "dimensions-disagree": "image-variant-unapproved",
  "available-missing-version": "shape",
  "missing-has-card_version": "image-variant-unapproved",
  "missing-has-art_version": "image-variant-unapproved",
  "missing-has-variants": "image-variant-unapproved",
  "pending-has-variants": "image-variant-unapproved",
  "duplicate-int-id": "primary-key-duplicate",
  "duplicate-face-ordinal": "face-ordinal",
  "same-name-face_id-f:a": "schema",
  "same-name-digital_phase-normal": "schema",
  "same-name-effect_similarity-reworked": "schema",
  "same-name-review_level-confirmed": "schema",
  "same-name-review_level-sampled": "schema",
  "same-name-duplicate-pair": "primary-key-duplicate",
  "same-name-human-shadow": "primary-key-duplicate",
  "endpoints-empty": "config-url-template",
  "endpoints-missing": "config-url-template",
  "endpoints-reverse": "config-url-template",
  "endpoints-refresh": "config-url-template",
  "media-dependencies-missing": "dependency-closure",
  "media-dependencies-extra": "dependency-closure",
}

function errorCode(run: () => unknown): ReaderErrorCode {
  try {
    run()
  } catch (error) {
    if (error instanceof SnapshotError) return error.code
    throw error
  }
  throw new Error("counterexample was accepted")
}

describe("official shared v3 contract inventory", () => {
  it("accounts for every indexed payload and every reader rejection", () => {
    const input = wire()
    expect(inventory["format_version"]).toBe("3.0.0")
    expect(inventory["bucket_count"]).toBe(64)
    expect(
      arrayValue(inventory["payloads"])
        .map((p) => stringValue(p))
        .sort(),
    ).toEqual(input.files.map((f) => `payloads/${stringValue(f["sha256"]).slice(7)}.json`).sort())
    expect(
      cases("reader_invalid")
        .map((c) => stringValue(c["name"]))
        .sort(),
    ).toEqual(Object.keys(expectedCodes).sort())
  })

  it("joins all individual payloads and text_all into the independent oracle", () => {
    const input = wire()
    const blobs = new Map([...input.payloads].map(([key, value]) => [key, canonical(value)]))
    const expected = canonicalText(v3Fixture(stringValue(inventory["expected"])))
    expect(canonicalText(readSnapshot(input.manifest, blobs))).toBe(expected)
    const attachments = new Map(
      input.files
        .filter((f) => f["role"] === "images" || f["role"] === "programs")
        .map((f) => [stringValue(f["key"]), blobs.get(stringValue(f["key"])) ?? new Uint8Array()]),
    )
    expect(
      canonicalText(
        readTextAll(
          input.manifest,
          canonical(v3Fixture(stringValue(inventory["text_all"]))),
          attachments,
        ),
      ),
    ).toBe(expected)
  })

  it.each(cases("reader_invalid"))("rejects reader case $name at its intended guard", (item) => {
    const input = mutatedWire(item)
    const blobs = reseal(input)
    expect(errorCode(() => readSnapshot(input.manifest, blobs))).toBe(
      expectedCodes[stringValue(item["name"])],
    )
  })

  it.each(
    cases("reader_invalid").filter((c) =>
      ["duplicate-int-id", "duplicate-face-ordinal"].includes(stringValue(c["name"])),
    ),
  )("rejects $name before startup adoption", async (item) => {
    const input = mutatedWire(item)
    const blobs = reseal(input)
    const manifestBytes = canonical(input.manifest)
    const manifestPath = `snapshots/manifests/${digest(manifestBytes).slice(7)}.json`
    const resources = new Map(
      input.files.map((f) => [
        stringValue(f["path"]),
        blobs.get(stringValue(f["key"])) ?? new Uint8Array(),
      ]),
    )
    resources.set(manifestPath, manifestBytes)
    const entry: JsonObject = {
      manifest_path: manifestPath,
      manifest_sha256: digest(manifestBytes),
    }
    const client = createSnapshotClient("https://cdn.test", {
      entry: "preview",
      fetch: (url) => {
        const path = url.slice("https://cdn.test/".length)
        const bytes =
          path === "snapshots/preview/current.json" ? canonical(entry) : resources.get(path)
        if (!bytes) throw new Error("unexpected startup fixture request")
        return Promise.resolve(new Response(bytes.slice().buffer))
      },
    })
    await client.load()
    expect(client.snapshot()).toBeNull()
    const status = client.status()
    expect(status).toMatchObject({ state: "error", kind: "corrupt" })
    if (status.state !== "error") throw new Error("invalid startup was adopted")
    expect(status.detail).toContain(expectedCodes[stringValue(item["name"])])
  })

  it.each(cases("schema_invalid"))("rejects schema case $name without scalar coercion", (item) => {
    const raw = item["raw_json"]
    const value = typeof raw === "string" ? (JSON.parse(raw) as JsonValue) : (item["value"] ?? null)
    expect(
      errorCode(() => {
        validate(stringValue(item["target"]), value, [], "3.0.0")
      }),
    ).toBe("schema")
    if (typeof raw === "string") expect(errorCode(() => parseStrict(utf8(raw)))).toBe("json")
  })

  it.each(cases("image_url_cases"))("executes URL vector $case without scalar coercion", (item) => {
    const raw = item["raw_json"]
    const args = typeof raw === "string" ? objectValue(JSON.parse(raw) as JsonValue) : item
    // Deliberately cross the typed API with invalid scalars to exercise the runtime boundary.
    const run = () =>
      imageUrl(
        "https://cdn.test",
        args["int_id"] as number,
        args["ordinal"] as number,
        args["size"] as string,
        args["version"] as number,
      )
    if (item["reject"] !== undefined) expect(errorCode(run)).toBe("image-variant-unapproved")
    else expect(run()).toBe(`https://cdn.test/${stringValue(item["url"])}`)
  })

  it.each(cases("index_cases"))("executes full Index selection case $name", async (item) => {
    const input = wire()
    const resources = new Map(
      input.files.map((f) => [
        stringValue(f["path"]),
        canonical(input.payloads.get(stringValue(f["key"])) ?? null),
      ]),
    )
    for (const [hash, manifest] of Object.entries(objectValue(item["manifests"]))) {
      const bytes = canonical(manifest)
      expect(digest(bytes)).toBe(hash)
      resources.set(`snapshots/manifests/${hash.slice(7)}.json`, bytes)
    }
    const requests: string[] = []
    const client = createSnapshotClient("https://cdn.test", {
      fetch: (url) => {
        const path = url.slice("https://cdn.test/".length)
        requests.push(path)
        const bytes =
          path === "snapshots/versions/index.json"
            ? canonical(item["index"] ?? null)
            : resources.get(path)
        if (!bytes) throw new Error("unexpected fixture request")
        return Promise.resolve(new Response(bytes.slice().buffer))
      },
    })
    await client.load()
    if (item["error"] !== undefined) {
      expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
      expect(requests).toEqual(["snapshots/versions/index.json"])
    } else if (item["selected_data_version"] === null) {
      expect(client.status()).toMatchObject({ state: "error", kind: "incompatible" })
      expect(requests).toEqual(["snapshots/versions/index.json"])
    } else {
      expect(client.status()).toMatchObject({
        state: "ready",
        dataVersion: item["selected_data_version"],
      })
      const index = objectValue(item["index"])
      const selected = objectValue(
        objectValue(index["current"])["data_version"] === item["selected_data_version"]
          ? index["current"]
          : index["previous"],
      )
      expect(requests.filter((p) => p.startsWith("snapshots/manifests/"))).toEqual([
        stringValue(selected["manifest_path"]),
      ])
      expect(requests.some((p) => p.includes("pages/"))).toBe(false)
    }
  })
})

describe("current translation wire", () => {
  it.each(
    cases("schema_invalid").filter((item) => stringValue(item["name"]).startsWith("Translation-")),
  )("rejects $name inside the full snapshot, including its resealed dependencies", (item) => {
    const input = wire()
    input.payloads.set("bootstrap/bootstrap/global/global/band/2", objectValue(item["value"]))
    expect(errorCode(() => readSnapshot(input.manifest, reseal(input)))).toBe("schema")
  })
  it("keeps the independently written low-confidence machine row in the joined text closure", () => {
    const input = wire()
    const view = readSnapshot(
      input.manifest,
      new Map([...input.payloads].map(([key, value]) => [key, canonical(value)])),
    )
    const row = view["translation"]?.find((value) => value["id"] === "tr:effect")
    expect(row).toMatchObject({ origin: "machine", authority: "unofficial", low_confidence: true })
    expect(row).not.toHaveProperty("status")
    expect(view["text_unit"]?.some((unit) => unit["id"] === row?.["source_unit_id"])).toBe(true)
    expect(view["text_unit"]?.some((unit) => unit["id"] === row?.["text_unit_id"])).toBe(true)
  })
})
