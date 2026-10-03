import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"

import { setActiveSnapshotRoot, type SnapshotClient, type SnapshotStatus } from "../data"
import { renderInRouter } from "../test-utils"
import { DevBadge } from "./DevBadge"

function fakeClient(
  status: SnapshotStatus,
): SnapshotClient & { retried: number; reload: () => Promise<void> } {
  const client = {
    base: "/cdn",
    retried: 0,
    status: () => status,
    subscribe: () => () => undefined,
    load: () => Promise.resolve(),
    retry: () => {
      client.retried += 1
      return Promise.resolve()
    },
    reload: () => Promise.resolve(),
    snapshot: () => null,
    metadataStatus: () => ({ state: "idle" as const, done: 0, total: 0, persistent: false }),
    prefetchImages: () => Promise.resolve(),
    cancelImagePrefetch: () => undefined,
    fragments: () => Promise.reject(new Error("no")),
  }
  return client
}

afterEach(() => {
  setActiveSnapshotRoot("cdn")
  vi.restoreAllMocks()
  vi.unstubAllEnvs()
})

describe("DevBadge", () => {
  it("shows the data version once ready", async () => {
    await renderInRouter(
      <DevBadge client={fakeClient({ state: "ready", dataVersion: "20260929T000000Z-0001" })} />,
    )
    expect(screen.getByRole("status")).toHaveTextContent("正式")
    expect(screen.getByRole("status")).toHaveTextContent("2026-09-29")
    expect(screen.getByRole("status")).toHaveAttribute("title", "20260929T000000Z-0001")
    expect(screen.queryByRole("button", { name: "重試" })).not.toBeInTheDocument()
  })

  it("names the loading phase and the error kind, and retries", async () => {
    const { unmount } = await renderInRouter(
      <DevBadge client={fakeClient({ state: "loading", phase: "manifest" })} />,
    )
    expect(screen.getByRole("status")).toHaveTextContent("載入中：快照清單")
    unmount()
    const client = fakeClient({ state: "error", kind: "incompatible", detail: "x" })
    render(<DevBadge client={client} />)
    expect(screen.getByRole("status")).toHaveTextContent("網站需要更新")
    await userEvent.click(screen.getByRole("button", { name: "重試" }))
    expect(client.retried).toBe(1)
  })

  it("shows a failed update on a still-ready client and reloads on retry", async () => {
    const client = fakeClient({
      state: "ready",
      dataVersion: "v1",
      updateError: { kind: "network", detail: "x" },
    })
    let reloaded = 0
    client.reload = () => {
      reloaded += 1
      return Promise.resolve()
    }
    await renderInRouter(<DevBadge client={client} />)
    expect(screen.getByRole("status")).toHaveTextContent("v1（更新失敗：網路失敗）")
    await userEvent.click(screen.getByRole("button", { name: "重試" }))
    expect(reloaded).toBe(1)
    expect(client.retried).toBe(0)
  })

  it("shows the reload phase next to the active version", async () => {
    await renderInRouter(
      <DevBadge client={fakeClient({ state: "ready", dataVersion: "v1", updating: "manifest" })} />,
    )
    expect(screen.getByRole("status")).toHaveTextContent("v1（更新中：快照清單）")
  })

  it("offers the preview root only when one is configured", async () => {
    vi.stubEnv("SVE_PREVIEW_CONFIGURED", "")
    await renderInRouter(<DevBadge client={fakeClient({ state: "idle" })} />)
    expect(screen.queryByRole("button", { name: "改用預覽資料" })).not.toBeInTheDocument()
  })

  it("flips between the formal and the preview root", async () => {
    vi.stubEnv("SVE_PREVIEW_CONFIGURED", "1")
    await renderInRouter(<DevBadge client={fakeClient({ state: "idle" })} />)
    await userEvent.click(screen.getByRole("button", { name: "改用預覽資料" }))
    expect(screen.getByRole("status")).toHaveTextContent("預覽")
    expect(screen.getByRole("button", { name: "改回正式資料" })).toBeInTheDocument()
  })
})
