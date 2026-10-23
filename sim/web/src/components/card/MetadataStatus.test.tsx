import { screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createSnapshotClient } from "../../data"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import { MetadataStatus } from "./MetadataStatus"

const built = await buildSnapshot({
  encodeImage: ({ width }) =>
    Promise.resolve(new TextEncoder().encode(`synthetic-${String(width)}`)),
})
afterEach(() => {
  prefsStore.set(DEFAULT_PREFS)
  vi.restoreAllMocks()
})

describe("metadata progress", () => {
  it.each([
    {
      language: "zh-TW" as const,
      button: "下載卡圖資訊",
      degraded: /無法使用持久快取/u,
    },
    {
      language: "ja" as const,
      button: "画像情報を取得",
      degraded: /永続キャッシュが使えません/u,
    },
    {
      language: "en" as const,
      button: "Download metadata",
      degraded: /Persistent cache unavailable/u,
    },
  ])(
    "starts background fetch on its own with a persistent cache and goes quiet when complete ($language)",
    async ({ language, button, degraded }) => {
      const client = createSnapshotClient("/cdn", {
        cacheStorage: memoryCache(),
        fetch: (url) => {
          const bytes = built.files.get(url.slice(5))
          return Promise.resolve(
            bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
          )
        },
      })
      await client.load()
      const start = vi.spyOn(client, "prefetchImages")
      await renderInRouter(<MetadataStatus client={client} />, { language })
      await waitFor(() => {
        expect(start).toHaveBeenCalledOnce()
      })
      expect(screen.queryByText(degraded)).not.toBeInTheDocument()
      // A finished download is not news: the row disappears instead of reporting completion.
      await waitFor(() => {
        expect(client.metadataStatus().state).toBe("complete")
      })
      await waitFor(() => {
        expect(screen.queryByRole("status")).not.toBeInTheDocument()
      })
      expect(screen.queryByRole("button", { name: button })).not.toBeInTheDocument()
    },
  )
  it.each(["zh-TW", "ja", "en"] as const)(
    "does not offer or start background work without persistent cache (%s)",
    async (language) => {
      const client = createSnapshotClient("/cdn", {
        fetch: (url) => {
          const bytes = built.files.get(url.slice(5))
          return Promise.resolve(
            bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
          )
        },
      })
      await client.load()
      const start = vi.spyOn(client, "prefetchImages")
      await renderInRouter(<MetadataStatus client={client} />, { language })
      expect(screen.queryByRole("button")).not.toBeInTheDocument()
      expect(start).not.toHaveBeenCalled()
    },
  )
})
