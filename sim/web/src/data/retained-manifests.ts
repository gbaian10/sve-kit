import { arrayValue, type JsonObject, objectValue, parseJson, stringValue } from "./format-v3/json"
import { transferDigest } from "./integrity"

const PREFIX = "sve-retained:"

/** Publish references only after adoption; failed writes leave the previous plans intact. */
export async function retainManifest(
  storage: CacheStorage,
  root: string,
  hash: string,
  files: ReadonlyMap<string, JsonObject>,
): Promise<void> {
  const cache = await storage.open(`${PREFIX}${encodeURIComponent(root)}`)
  const key = `${root}/__sve-retained/${encodeURIComponent(hash)}`
  // Recency, not insertion order, decides the two kept plans: re-adopting an older hash makes it current.
  const records = async () => {
    const dated = await Promise.all(
      (await cache.keys()).map(async (request) => ({
        url: request.url,
        touched: Number((await cache.match(request))?.headers.get("x-sve-touched")) || 0,
      })),
    )
    return dated.sort((a, b) => a.touched - b.touched)
  }
  const before = await records()
  const newest = before.at(-1)
  const existing = await cache.match(key)
  const intact =
    existing &&
    (await transferDigest(new Uint8Array(await existing.arrayBuffer()))) ===
      existing.headers.get("x-sve-sha256")
  // An intact newest plan needs no write; anything else (older, damaged) is rewritten as the newest.
  if (!(intact && newest && newest.url === new Request(key).url)) {
    const payload = new TextEncoder().encode(
      JSON.stringify({
        hash,
        hashes: [
          ...new Set([hash, ...[...files.values()].map((file) => stringValue(file["sha256"]))]),
        ],
      }),
    )
    await cache.put(
      key,
      new Response(payload, {
        headers: {
          "x-sve-sha256": await transferDigest(payload),
          "x-sve-bytes": String(payload.length),
          "x-sve-touched": String(Math.max(Date.now(), (newest?.touched ?? 0) + 1)),
        },
      }),
    )
  }
  // No cross-tab lock: a concurrent removal is an ordinary verified-cache miss.
  const kept = await records()
  for (const old of kept.slice(0, Math.max(0, kept.length - 2))) await cache.delete(old.url)
}

export async function retainedReferences(storage: CacheStorage, names: readonly string[]) {
  const roots = new Map<string, { manifests: Set<string>; raw: Set<string> }>()
  for (const name of names.filter((name) => name.startsWith(PREFIX))) {
    const refs = { manifests: new Set<string>(), raw: new Set<string>() }
    const cache = await storage.open(name)
    for (const key of await cache.keys()) {
      const response = await cache.match(key)
      if (!response) continue
      const payload = new Uint8Array(await response.arrayBuffer())
      if ((await transferDigest(payload)) !== response.headers.get("x-sve-sha256"))
        throw new Error("Retained manifest references damaged")
      const plan = objectValue(parseJson(payload))
      refs.manifests.add(stringValue(plan["hash"]))
      for (const hash of arrayValue(plan["hashes"])) refs.raw.add(stringValue(hash))
    }
    if (refs.manifests.size) roots.set(name.slice(PREFIX.length), refs)
  }
  return roots
}
