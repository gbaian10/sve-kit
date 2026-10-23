import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createSnapshotClient } from "../../data"
import { renderInRouter } from "../../test-utils"
import { memoryCache } from "../../test-utils/cache"
import { CardQa } from "./CardQa"

async function loadedClient() {
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
  return client
}

it("shows a translated entry with its source, and falls back to the original with a note", async () => {
  const client = await loadedClient()
  const { unmount } = await renderInRouter(
    <CardQa client={client} cardId="c:bp01-002" edition="jp" showOther={false} />,
  )
  const translated = await screen.findByRole("listitem", { name: "Q001" })
  expect(
    await within(translated).findByText("有兩張試作隊長時，士兵攻擊力是 +2 嗎？"),
  ).toBeVisible()
  expect(await within(translated).findAllByText(/專案譯文/u)).toHaveLength(1)
  await userEvent.click(within(translated).getByText("顯示原文"))
  expect(within(translated).getByText(/試作の隊長が2体いる時/u)).toBeVisible()
  expect(within(translated).queryByText("未翻譯")).toBeNull()
  unmount()

  await renderInRouter(
    <CardQa client={client} cardId="c:bp01-011" edition="jp" showOther={false} />,
  )
  const missing = await screen.findByRole("listitem", { name: "Q010" })
  expect(await within(missing).findByText(/フェアリーが進化していても/u)).toBeVisible()
  expect(await within(missing).findAllByText("未翻譯")).toHaveLength(2)
  expect(within(missing).queryByText("顯示原文")).toBeNull()
  // Two entries stay under the fold limit, so there is nothing to expand.
  expect(screen.queryByRole("button", { name: /顯示全部/u })).toBeNull()
})
