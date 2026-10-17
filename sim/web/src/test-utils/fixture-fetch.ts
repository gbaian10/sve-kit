import { vi } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"

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
    const bytes = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
}
