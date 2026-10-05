interface ObjectMetadata {
  httpEtag: string
  httpMetadata?: {
    contentType?: string
    contentEncoding?: string
    cacheControl?: string
  }
}

interface ReadObject extends ObjectMetadata {
  body: ReadableStream<Uint8Array>
}

// Structural binding types keep the Worker independent of browser and generated runtime globals.
interface Env {
  PREVIEW_BUCKET: {
    get(key: string): Promise<ReadObject | null>
    head(key: string): Promise<ObjectMetadata | null>
  }
  ASSETS: { fetch(request: Request): Promise<Response> }
}

const PREFIX = "/cdn-preview/"
const INDEX = "snapshots/versions/index.json"
const JSON_KEY = /^snapshots\/(?:blobs|manifests)\/[0-9a-f]{64}\.json(?:\.(?:br|gz))?$/
const IMAGE_KEY = /^images\/(?:card_[sml]|art_[sm])\/([1-9][0-9]*)(?:-f([1-9][0-9]*))?\.webp$/

function positiveInteger(value: string | null): boolean {
  if (value === null) return false
  const number = Number(value)
  return Number.isSafeInteger(number) && number > 0 && String(number) === value
}

function allowedKey(key: string): boolean {
  // Public 2.0 keys need no decoding; rejecting escapes also rejects encoded separators and traversal.
  if (/[^a-z0-9_./-]/.test(key)) return false
  if (key === INDEX || JSON_KEY.test(key)) return true
  const image = IMAGE_KEY.exec(key)
  return (
    image !== null &&
    positiveInteger(image[1] ?? null) &&
    (image[2] === undefined || positiveInteger(image[2]))
  )
}

function cacheControl(value: string | undefined): string {
  // The publisher owns cache policy; Access-protected responses must not enter shared caches.
  return value?.replace(/(^|,)(\s*)public(?=\s*(?:,|$))/gi, "$1$2private") ?? "private, no-cache"
}

function matchesEtag(condition: string | null, etag: string): boolean {
  if (condition === null) return false
  if (condition.trim() === "*") return true
  const tags = condition.matchAll(/(?:^|,)\s*(?:W\/)?("[^"]*")\s*(?=,|$)/g)
  return Array.from(tags).some((tag) => tag[1] === etag)
}

function failure(status: number): Response {
  const headers = new Headers({ "Cache-Control": "no-store" })
  if (status === 405) headers.set("Allow", "GET, HEAD")
  return new Response(null, { status, headers })
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url)
    // Disabled aliases are also denied here if a deployment still exposes one.
    if (url.origin !== "https://dev.svekit.app") return failure(404)

    // URL.pathname normalizes dot segments; check the incoming spelling before that normalization.
    const pathname = /^https?:\/\/[^/?#]+([^?#]*)/.exec(request.url)?.[1] ?? ""
    const dataPath =
      pathname === "/cdn-preview" ||
      pathname.startsWith(PREFIX) ||
      /^\/cdn-preview[%\\]/.test(pathname)
    if (!dataPath) return env.ASSETS.fetch(request)
    if (request.method !== "GET" && request.method !== "HEAD") return failure(405)
    if (!pathname.startsWith(PREFIX)) return failure(404)
    const key = pathname.slice(PREFIX.length)
    if (!allowedKey(key)) return failure(404)

    try {
      let object: ObjectMetadata | null
      let body: ReadObject["body"] | null = null
      if (request.method === "HEAD") {
        object = await env.PREVIEW_BUCKET.head(key)
      } else {
        const result = await env.PREVIEW_BUCKET.get(key)
        object = result
        body = result?.body ?? null
      }
      if (object === null) return failure(404)

      const headers = new Headers({
        ETag: object.httpEtag,
        "Cache-Control": cacheControl(object.httpMetadata?.cacheControl),
      })
      const metadata = object.httpMetadata
      if (metadata?.contentType !== undefined) headers.set("Content-Type", metadata.contentType)
      if (metadata?.contentEncoding !== undefined) {
        headers.set("Content-Encoding", metadata.contentEncoding)
      }
      if (matchesEtag(request.headers.get("If-None-Match"), object.httpEtag)) {
        await body?.cancel()
        return new Response(null, { status: 304, headers })
      }

      // R2 compressed siblings already contain wire bytes; Workers must not compress them again.
      const init: ResponseInit & { encodeBody: "manual" } = { headers, encodeBody: "manual" }
      return new Response(body, init)
    } catch {
      return failure(502)
    }
  },
}
