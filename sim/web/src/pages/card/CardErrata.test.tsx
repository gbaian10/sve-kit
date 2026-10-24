import { render, screen, within } from "@testing-library/react"
import { expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { InlineText } from "../../components/card/InlineText"
import { createSnapshotClient, loadCardErrata } from "../../data"
import { renderInRouter } from "../../test-utils"
import { annotatedOrigin } from "../../test-utils/annotated-snapshot"
import { memoryCache } from "../../test-utils/cache"
import { CardErrata } from "./CardErrata"

it("shows reconstructed full text, excerpt translations and source while keeping printing unconfirmed", async () => {
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
  await renderInRouter(
    <CardErrata
      client={client}
      cardId="c:bp01-020"
      printingId="p:bp01-020"
      faceId="f:bp01-020"
      region="jp"
    />,
  )
  const notice = await screen.findByRole("region", { name: "勘誤" })
  expect(notice.querySelector("time")).toBeNull()
  expect(within(notice).queryByText(/2026-03-01/u)).not.toBeInTheDocument()
  expect(within(notice).getByRole("link", { name: "官方勘誤公告" })).toHaveAttribute(
    "href",
    "https://example.invalid/errata/bp01-020",
  )
  expect(notice.querySelectorAll("del")).toHaveLength(2)
  expect(within(notice).getAllByText("繁中・日文來源機器譯文（非官方）")).toHaveLength(1)
  const translation = notice.querySelector('[lang="zh-Hant"]')
  const original = notice.querySelector('[lang="ja"]')
  expect(
    translation &&
      original &&
      original.compareDocumentPosition(translation) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(
    notice.querySelector('del [lang="zh-Hant"]') ?? notice.querySelector('[lang="zh-Hant"] del'),
  ).toHaveTextContent("2")
  expect(within(notice).getByRole("heading", { name: "實體卡印刷文字" })).toBeVisible()
  expect(within(notice).getByRole("heading", { name: "勘誤後完整文字" })).toBeVisible()
  expect(within(notice).getByText(/此版次是否印有舊文尚未確認/u)).toBeVisible()
  expect(within(notice).getByText(/BP01-020（印刷文字未確認）/u)).toBeVisible()
})

it("emphasizes explicit keyword headings without inventing historical annotation references", () => {
  const { container } = render(<InlineText text="【連擊 5】改為選擇。" emphasis />)
  expect(container.querySelector("strong")).toHaveTextContent("【連擊 5】")
})

it("returns no notices for an edition absent from the snapshot", async () => {
  const { client } = await annotatedOrigin()
  await client.load()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  await expect(loadCardErrata(client, snapshot, "card", "printing", "face", "en")).resolves.toEqual(
    [],
  )
})
