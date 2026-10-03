import { screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
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
      complete: "卡圖資訊已下載",
    },
    {
      language: "ja" as const,
      button: "画像情報を取得",
      degraded: /永続キャッシュが使えません/u,
      complete: "画像情報を取得済み",
    },
    {
      language: "en" as const,
      button: "Download metadata",
      degraded: /Persistent cache unavailable/u,
      complete: "Image metadata downloaded",
    },
  ])(
    "data saver delays background fetch with retry and cache degradation ($language)",
    async ({ language, button, degraded, complete }) => {
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
      prefsStore.set({ ...DEFAULT_PREFS, dataSaver: true })
      await renderInRouter(<MetadataStatus client={client} />, { language })
      expect(start).not.toHaveBeenCalled()
      expect(screen.queryByText(degraded)).not.toBeInTheDocument()
      await userEvent.setup().click(screen.getByRole("button", { name: button }))
      expect(start).toHaveBeenCalledOnce()
      await waitFor(() => {
        expect(screen.getByRole("status")).toHaveTextContent(complete)
      })
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
