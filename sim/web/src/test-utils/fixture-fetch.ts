import { vi } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"

type Route = (url: string) => Promise<Response> | undefined

let extraRoute: Route | undefined

/** Lets a test answer some URLs itself (an API, say); `undefined` hands them back to the fixture. */
export function routeFixtureFetch(route: Route | undefined) {
  extraRoute = route
}

/** Builds the synthetic snapshot once and answers `fetch` from it, like the page tests do. */
export async function stubFixtureFetch() {
  const built = await buildSnapshot({
    encodeImage: ({ width, height, seed }) =>
      Promise.resolve(
        new TextEncoder().encode(`stub:${String(width)}x${String(height)}:${String(seed)}`),
      ),
  })
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const routed = extraRoute?.(url)
    if (routed) return routed
    const bytes = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
}
