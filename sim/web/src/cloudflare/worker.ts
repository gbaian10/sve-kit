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

interface Context {
  waitUntil(promise: Promise<unknown>): void
}

interface EdgeCache {
  match(request: Request): Promise<Response | undefined>
  put(request: Request, response: Response): Promise<void>
}

const PREFIX = "/cdn-preview/"
const INDEX = "snapshots/versions/index.json"
const JSON_KEY = /^snapshots\/(?:blobs|manifests)\/[0-9a-f]{64}\.json(?:\.(?:br|gz))?$/
const IMAGE_KEY = /^images\/(?:card_[sml]|art_[sm])\/([1-9][0-9]*)(?:-f([1-9][0-9]*))?\.webp$/
const BROWSER_CACHE_CONTROL = "X-Sve-Browser-Cache-Control"
const EDGE_CACHE_CONTROL = "public, max-age=31536000, immutable"
// Vite names its build output by content hash, so a reload need not revalidate these files.
const HASHED_ASSET = /^\/assets\/[A-Za-z0-9_.-]+-[A-Za-z0-9_-]{8}\.[a-z0-9]+$/
const HASHED_ASSET_CACHE_CONTROL = "private, max-age=31536000, immutable"

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

async function asset(request: Request, env: Env, pathname: string): Promise<Response> {
  const response = await env.ASSETS.fetch(request)
  // The single-page fallback answers a missing file with HTML, which must never be pinned.
  const html = response.headers.get("Content-Type")?.startsWith("text/html") ?? false
  if (!HASHED_ASSET.test(pathname) || html || (response.status !== 200 && response.status !== 304))
    return response
  const headers = new Headers(response.headers)
  headers.set("Cache-Control", HASHED_ASSET_CACHE_CONTROL)
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  })
}

function failure(status: number): Response {
  const headers = new Headers({ "Cache-Control": "no-store" })
  if (status === 405) headers.set("Allow", "GET, HEAD")
  return new Response(null, { status, headers })
}

async function clientResponse(
  request: Request,
  body: ReadObject["body"] | null,
  headers: Headers,
): Promise<Response> {
  if (matchesEtag(request.headers.get("If-None-Match"), headers.get("ETag") ?? "")) {
    await body?.cancel()
    return new Response(null, { status: 304, headers })
  }
  if (request.method === "HEAD") {
    await body?.cancel()
    body = null
  }
  // R2 compressed siblings already contain wire bytes; Workers must not compress them again.
  const init: ResponseInit & { encodeBody: "manual" } = { headers, encodeBody: "manual" }
  return new Response(body, init)
}

async function store(cache: EdgeCache, key: Request, response: Response): Promise<void> {
  try {
    await cache.put(key, response)
  } catch {
    // A failed write only loses a future hit; release the tee branch so it stops buffering.
    await response.body?.cancel().catch(() => undefined)
  }
}

export default {
  async fetch(request: Request, env: Env, ctx: Context): Promise<Response> {
    const url = new URL(request.url)
    // Disabled aliases are also denied here if a deployment still exposes one.
    if (url.origin !== "https://dev.svekit.app") return failure(404)

    // URL.pathname normalizes dot segments; check the incoming spelling before that normalization.
    const pathname = /^https?:\/\/[^/?#]+([^?#]*)/.exec(request.url)?.[1] ?? ""
    const dataPath =
      pathname === "/cdn-preview" ||
      pathname.startsWith(PREFIX) ||
      /^\/cdn-preview[%\\]/.test(pathname)
    if (!dataPath) return asset(request, env, pathname)
    if (request.method !== "GET" && request.method !== "HEAD") return failure(405)
    if (!pathname.startsWith(PREFIX)) return failure(404)
    const key = pathname.slice(PREFIX.length)
    if (!allowedKey(key)) return failure(404)

    try {
      let edgeCache: EdgeCache | undefined
      let cached: Response | undefined
      // Only immutable paths can share a GET entry across queries, cookies and HEAD requests.
      const cacheKey = JSON_KEY.test(key) ? new Request(`${url.origin}${PREFIX}${key}`) : null
      if (cacheKey !== null) {
        try {
          edgeCache = (globalThis as typeof globalThis & { caches: { default: EdgeCache } }).caches
            .default
          cached = await edgeCache.match(cacheKey)
        } catch {
          // Cache availability must not prevent authorized R2 reads (including behind Access).
          edgeCache = undefined
        }
      }
      if (cached !== undefined) {
        const headers = new Headers(cached.headers)
        headers.set("Cache-Control", headers.get(BROWSER_CACHE_CONTROL) ?? "private, no-cache")
        headers.delete(BROWSER_CACHE_CONTROL)
        return await clientResponse(request, cached.body, headers)
      }

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
      const response = await clientResponse(request, body, headers)
      if (
        cacheKey !== null &&
        edgeCache !== undefined &&
        request.method === "GET" &&
        response.status === 200
      ) {
        const copy = response.clone()
        const cacheHeaders = new Headers(copy.headers)
        // Public policy exists only on the internal entry; every browser response stays private.
        cacheHeaders.set(BROWSER_CACHE_CONTROL, headers.get("Cache-Control") ?? "private, no-cache")
        cacheHeaders.set("Cache-Control", EDGE_CACHE_CONTROL)
        const init: ResponseInit & { encodeBody: "manual" } = {
          headers: cacheHeaders,
          encodeBody: "manual",
        }
        ctx.waitUntil(store(edgeCache, cacheKey, new Response(copy.body, init)))
      }
      return response
    } catch {
      return failure(502)
    }
  },
}
