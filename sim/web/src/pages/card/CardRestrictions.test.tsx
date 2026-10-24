import { screen, waitFor } from "@testing-library/react"
import { expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { createSnapshotClient } from "../../data"
import { renderInRouter } from "../../test-utils"
import { CardRestrictions } from "./CardRestrictions"

it("shows the source list and effective date, then drops old data when switching regions", async () => {
  const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array()) })
  const client = createSnapshotClient("https://fixture.invalid", {
    fetch: (url) => {
      const bytes = built.files.get(url.replace("https://fixture.invalid/", ""))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await client.load()
  const rendered = await renderInRouter(
    <CardRestrictions client={client} cardId="c:bp01-021" region="jp" />,
  )
  expect(await screen.findByText("殿堂入り（限 1 張）")).toBeVisible()
  expect(screen.getByText("2026-04-01 生效")).toBeVisible()
  expect(screen.getByRole("link", { name: "合成一覽連結" })).toHaveAttribute(
    "href",
    "https://example.invalid/restrictions",
  )
  rendered.rerender(<CardRestrictions client={client} cardId="c:bp01-021" region="en" />)
  await waitFor(() => expect(screen.queryByText("殿堂入り（限 1 張）")).not.toBeInTheDocument())
  expect(await screen.findByText("此地區的禁限一覽尚未確認。")).toBeVisible()
})
