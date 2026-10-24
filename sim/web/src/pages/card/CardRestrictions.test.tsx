import { act, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import * as data from "../../data"
import { renderInRouter } from "../../test-utils"
import { CardRestrictions } from "./CardRestrictions"

const restrictionLoads = new Map<string, Promise<void>>()
const restrictionResults = new Map<string, data.CardRestrictionDetails>()
const restrictionLoadKey = (cardId: string, region: string) => JSON.stringify([cardId, region])

function trackRestrictionLoads() {
  const loadDetails = data.loadCardRestrictionDetails
  vi.spyOn(data, "loadCardRestrictionDetails").mockImplementation(async (...args) => {
    let finishLoad: (() => void) | undefined
    const loaded = new Promise<void>((resolve) => {
      finishLoad = resolve
    })
    const key = restrictionLoadKey(args[2], args[3])
    restrictionLoads.set(key, loaded)
    try {
      const result = await loadDetails(...args)
      restrictionResults.set(key, result)
      return result
    } finally {
      finishLoad?.()
    }
  })
}

async function waitForRestrictionLoad(cardId: string, region: string) {
  const loaded = restrictionLoads.get(restrictionLoadKey(cardId, region))
  if (!loaded) throw new Error("restriction load was not started")
  await act(async () => {
    await loaded
    await Promise.resolve()
  })
}

afterEach(() => {
  restrictionLoads.clear()
  restrictionResults.clear()
  vi.restoreAllMocks()
})

async function createClient(restrictionState?: "partial") {
  const built = await buildSnapshot({
    encodeImage: () => Promise.resolve(new Uint8Array()),
    ...(restrictionState ? { restrictionCoverageState: restrictionState } : {}),
  })
  const files = built.files

  let failDetailRequests = false
  const client = data.createSnapshotClient("https://fixture.invalid", {
    fetch: (url) => {
      if (failDetailRequests && url.includes("snapshots/blobs/"))
        return Promise.resolve(new Response(null, { status: 404 }))
      const bytes = files.get(url.replace("https://fixture.invalid/", ""))
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  await client.load()
  return {
    client,
    failDetailRequests: (fail: boolean) => {
      failDetailRequests = fail
    },
  }
}

it("shows restrictions with dates and source, then drops old data when switching regions", async () => {
  const { client } = await createClient()
  trackRestrictionLoads()
  const rendered = await renderInRouter(
    <CardRestrictions client={client} cardId="c:bp01-021" region="jp" />,
  )
  await waitForRestrictionLoad("c:bp01-021", "jp")

  expect(screen.getByText("殿堂入り（限 1 張）")).toBeVisible()
  expect(screen.getByText("2026-04-01 生效")).toBeVisible()
  expect(screen.getByRole("link", { name: "合成一覽連結" })).toHaveAttribute(
    "href",
    "https://example.invalid/restrictions",
  )

  rendered.rerender(<CardRestrictions client={client} cardId="c:bp01-021" region="en" />)
  await waitForRestrictionLoad("c:bp01-021", "en")
  expect(screen.queryByText("殿堂入り（限 1 張）")).not.toBeInTheDocument()
  expect(screen.queryByRole("region", { name: "禁限狀態" })).toBeNull()
})

it("hides a complete list when the card has no restriction", async () => {
  const { client } = await createClient()
  trackRestrictionLoads()
  await renderInRouter(<CardRestrictions client={client} cardId="c:bp01-001" region="jp" />)
  await waitForRestrictionLoad("c:bp01-001", "jp")

  expect(restrictionResults.get(restrictionLoadKey("c:bp01-001", "jp"))?.status.state).toBe(
    "complete",
  )
  expect(screen.queryByRole("region", { name: "禁限狀態" })).toBeNull()
})

it("hides the panel when restriction status is unknown", async () => {
  const { client } = await createClient()
  trackRestrictionLoads()
  await renderInRouter(<CardRestrictions client={client} cardId="c:bp01-001" region="en" />)
  await waitForRestrictionLoad("c:bp01-001", "en")

  expect(restrictionResults.get(restrictionLoadKey("c:bp01-001", "en"))?.status.state).toBe(
    "unknown",
  )
  expect(screen.queryByRole("region", { name: "禁限狀態" })).toBeNull()
})

it("hides a partial list when the card has no known restriction", async () => {
  const { client } = await createClient("partial")
  trackRestrictionLoads()
  await renderInRouter(<CardRestrictions client={client} cardId="c:bp01-001" region="jp" />)
  await waitForRestrictionLoad("c:bp01-001", "jp")

  expect(restrictionResults.get(restrictionLoadKey("c:bp01-001", "jp"))?.status.state).toBe(
    "partial",
  )
  expect(restrictionResults.get(restrictionLoadKey("c:bp01-001", "jp"))?.restrictions).toHaveLength(
    0,
  )
  expect(screen.queryByRole("region", { name: "禁限狀態" })).toBeNull()
  expect(screen.queryByText("只列已知限制；來源尚有缺口")).not.toBeInTheDocument()
})

it("shows a known restriction and partial-list notice", async () => {
  const { client } = await createClient("partial")
  trackRestrictionLoads()
  await renderInRouter(<CardRestrictions client={client} cardId="c:bp01-021" region="jp" />)
  await waitForRestrictionLoad("c:bp01-021", "jp")

  expect(screen.getByText("殿堂入り（限 1 張）")).toBeVisible()
  expect(screen.getByText("只列已知限制；來源尚有缺口")).toBeVisible()
})

it("keeps the retry button visible when restriction data fails to load", async () => {
  const { client, failDetailRequests } = await createClient()
  failDetailRequests(true)
  trackRestrictionLoads()
  await renderInRouter(<CardRestrictions client={client} cardId="c:bp01-021" region="jp" />)

  expect(await screen.findByRole("button", { name: "重試禁限資料" })).toBeVisible()
})
