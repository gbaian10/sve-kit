import fixtureJson from "../../../../tests/fixtures/snapshot-contract/v3/annotated-native.json"
import {
  canonicalText,
  createSnapshotClient,
  type JsonObject,
  objectValue,
  stringValue,
} from "../data"

const canonical = (value: Parameters<typeof canonicalText>[0]) =>
  new TextEncoder().encode(canonicalText(value))

export async function annotatedOrigin() {
  const fixture = objectValue(fixtureJson)
  const manifest = objectValue(fixture["manifest"])
  const payloads = new Map(
    Object.entries(objectValue(fixture["payloads"])).map(([key, value]) => [key, canonical(value)]),
  )
  const files = new Map(
    (manifest["files"] as JsonObject[]).map((file) => [
      stringValue(file["path"]),
      payloads.get(stringValue(file["key"])) ?? new Uint8Array(),
    ]),
  )
  const raw = canonical(manifest)
  const hash =
    "sha256:" +
    Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", raw)), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("")
  const path = `snapshots/manifests/${hash.slice(7)}.json`
  files.set(path, raw)
  files.set(
    "snapshots/preview/current.json",
    canonical({ manifest_path: path, manifest_sha256: hash }),
  )
  const requests: string[] = []
  const client = createSnapshotClient("/cdn", {
    entry: "preview",
    fetch: (url) => {
      const path = url.slice("/cdn/".length)
      requests.push(path)
      const bytes = files.get(path)
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  return { fixture, manifest, payloads, client, requests }
}
