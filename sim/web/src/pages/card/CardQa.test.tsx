import { isInaccessible } from "@testing-library/dom"
import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createSnapshotClient, stringValue } from "../../data"
import { prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import { CardQa } from "./CardQa"

async function loadedClient(disconnectAfterList = false, mixedSources = false) {
  const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
  const client = createSnapshotClient("https://cdn.test", {
    region: "jp",
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const bytes = built.files.get(url.slice("https://cdn.test/".length))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await client.load()
  let offline = false
  return {
    client: {
      ...client,
      fragments: async (...args: Parameters<typeof client.fragments>) => {
        if (offline) throw new TypeError("network unavailable")
        const fragments = await client.fragments(...args)
        // Lose access after the list's reads finish, before the text resolver reads its owners.
        if (disconnectAfterList && fragments.some((fragment) => fragment.table === "qa_version"))
          offline = true
        return mixedSources
          ? fragments.map((fragment) =>
              fragment.table === "translation"
                ? {
                    ...fragment,
                    rows: fragment.rows.map((row) =>
                      stringValue(row["id"]).includes(":answer:")
                        ? { ...row, origin: "machine", low_confidence: true }
                        : row,
                    ),
                  }
                : fragment,
            )
          : fragments
      },
    },
    reconnect: () => {
      offline = false
      disconnectAfterList = false
    },
  }
}

it("shows a translated entry with its source, and falls back to the original with a note", async () => {
  const { client, reconnect } = await loadedClient(true)
  const { unmount } = await renderInRouter(
    <CardQa client={client} cardId="c:bp01-002" edition="jp" showOther={false} />,
  )
  const translated = await screen.findByRole("listitem", { name: "Q001" })
  await waitFor(() => {
    expect(within(translated).getAllByRole("button", { name: "重新載入 Q&A" })).toHaveLength(2)
  })
  reconnect()
  for (const button of within(translated).getAllByRole("button", { name: "重新載入 Q&A" }))
    await userEvent.click(button)
  expect(
    await within(translated).findByText("有兩張試作隊長時，士兵攻擊力是 +2 嗎？"),
  ).toBeVisible()
  expect(isInaccessible(within(translated).getByText("問", { ignore: "details span" }))).toBe(false)
  expect(isInaccessible(within(translated).getByText("答", { ignore: "details span" }))).toBe(false)
  expect(await within(translated).findAllByText(/專案譯文/u)).toHaveLength(1)
  expect(within(translated).getByText(/試作の隊長が2体いる時/u)).toBeVisible()
  expect(within(translated).queryByText("未翻譯")).toBeNull()
  expect(within(translated).queryByText("待校對")).toBeNull()
  act(() => {
    prefsStore.set({ qaDisplay: "translated" })
  })
  expect(within(translated).queryByText(/試作の隊長が2体いる時/u)).toBeNull()
  expect(within(translated).getByText("有兩張試作隊長時，士兵攻擊力是 +2 嗎？")).toBeVisible()
  act(() => {
    prefsStore.set({ qaDisplay: "original" })
  })
  expect(within(translated).getByText(/試作の隊長が2体いる時/u)).toBeVisible()
  expect(within(translated).queryByText("有兩張試作隊長時，士兵攻擊力是 +2 嗎？")).toBeNull()
  expect(within(translated).queryByText(/專案譯文/u)).toBeNull()
  act(() => {
    prefsStore.set({ qaDisplay: "none" })
  })
  expect(screen.queryByRole("region", { name: "Q&A" })).toBeNull()
  act(() => {
    prefsStore.set({ qaDisplay: "both" })
  })
  unmount()

  await renderInRouter(
    <CardQa client={client} cardId="c:bp01-011" edition="jp" showOther={false} />,
  )
  const missing = await screen.findByRole("listitem", { name: "Q010" })
  expect(await within(missing).findByText(/フェアリーが進化していても/u)).toBeVisible()
  expect(await within(missing).findAllByText("未翻譯")).toHaveLength(2)
  expect(
    within(missing)
      .getAllByText("未翻譯")
      .map((note) => note.closest("[lang]")),
  ).toEqual([null, null])
  // Two entries stay under the fold limit, so there is nothing to expand.
  expect(screen.queryByRole("button", { name: /顯示全部/u })).toBeNull()
})

it("only attributes visible translations when the question and answer have different sources", async () => {
  const { client } = await loadedClient(false, true)
  await renderInRouter(
    <CardQa client={client} cardId="c:bp01-002" edition="jp" showOther={false} />,
  )
  const entry = await screen.findByRole("listitem", { name: "Q001" })
  expect(await within(entry).findByText(/專案譯文/u)).toBeVisible()
  expect(await within(entry).findByText(/機器譯文/u)).toBeVisible()
  expect(within(entry).queryByText("待校對")).toBeNull()
  act(() => {
    prefsStore.set({ qaDisplay: "original" })
  })
  expect(within(entry).getByText(/試作の隊長が2体いる時/u)).toBeVisible()
  expect(within(entry).queryByText(/專案譯文|機器譯文/u)).toBeNull()
  act(() => {
    prefsStore.set({ qaDisplay: "translated" })
  })
  expect(within(entry).getByText(/專案譯文/u)).toBeVisible()
  expect(within(entry).getByText(/機器譯文/u)).toBeVisible()
  expect(within(entry).queryByText("待校對")).toBeNull()
  expect(within(entry).queryByText(/試作の隊長が2体いる時/u)).toBeNull()
})
