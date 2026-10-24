import { isInaccessible } from "@testing-library/dom"
import { act, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { arrayValue, createSnapshotClient, objectValue, stringValue } from "../../data"
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
  const tab = await screen.findByRole("tab", { name: "Q001" })
  expect(tab).toHaveAttribute("aria-selected", "true")
  const panel = screen.getByRole("tabpanel")
  expect(tab).toHaveAttribute("aria-controls", panel.id)
  expect(panel).toHaveAttribute("aria-labelledby", tab.id)
  const translated = await within(panel).findByRole("listitem", { name: "Q001" })
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
  expect(within(translated).getByText("有兩張試作隊長時，士兵攻擊力是 +2 嗎？")).toBeVisible()
  expect(within(translated).queryByText(/試作の隊長が2体いる時/u)).toBeNull()
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
  const qaTabs = await screen.findAllByRole("tab")
  expect(qaTabs.map((item) => item.textContent)).toEqual(["Q010", "Q011"])
  expect(qaTabs[0]).toHaveAttribute("aria-selected", "true")
  expect(qaTabs[1]).toHaveAttribute("aria-selected", "false")
  const firstPanel = screen.getByRole("tabpanel")
  const missing = await within(firstPanel).findByRole("listitem", { name: "Q010" })
  expect(await within(missing).findByText(/フェアリーが進化していても/u)).toBeVisible()
  expect(await within(missing).findAllByText("未翻譯")).toHaveLength(2)
  expect(
    within(missing)
      .getAllByText("未翻譯")
      .map((note) => note.closest("[lang]")),
  ).toEqual([null, null])
  const secondTab = screen.getByRole("tab", { name: "Q011" })
  await userEvent.click(secondTab)
  expect(secondTab).toHaveAttribute("aria-selected", "true")
  expect(screen.getByRole("tab", { name: "Q010" })).toHaveAttribute("aria-selected", "false")
  expect(within(screen.getByRole("tabpanel")).getByRole("listitem", { name: "Q011" })).toBeVisible()
})

it("shows a single Q&A in its own selected tab", async () => {
  const { client } = await loadedClient()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("synthetic snapshot did not load")
  const single = {
    ...snapshot,
    manifest: {
      ...snapshot.manifest,
      qa_card_index: {
        jp: { "c:bp01-002": ["qa:bp01-002-1:v1"] },
        en: {},
      },
    },
  }
  await renderInRouter(
    <CardQa
      client={{ ...client, snapshot: () => single }}
      cardId="c:bp01-002"
      edition="jp"
      showOther={false}
    />,
  )
  const tab = await screen.findByRole("tab", { name: "Q001" })
  expect(tab).toHaveAttribute("aria-selected", "true")
  expect(
    await within(screen.getByRole("tabpanel")).findByRole("listitem", { name: "Q001" }),
  ).toBeVisible()
})

it("uses an ordinal tab label and focusable panel when official metadata is empty", async () => {
  const { client } = await loadedClient()
  const withoutNumber = {
    ...client,
    fragments: async (...args: Parameters<typeof client.fragments>) => {
      const fragments = await client.fragments(...args)
      return fragments.map((fragment) =>
        fragment.table === "qa"
          ? {
              ...fragment,
              rows: fragment.rows.map((row) =>
                stringValue(row["id"]) === "qa:bp01-002-1"
                  ? { ...row, official_number: "", source_url: null }
                  : row,
              ),
            }
          : fragment,
      )
    },
  }
  await renderInRouter(
    <CardQa client={withoutNumber} cardId="c:bp01-002" edition="jp" showOther={false} />,
  )
  expect(await screen.findByRole("tab", { name: "Q&A 1" })).toHaveAttribute("aria-selected", "true")
  const panel = screen.getByRole("tabpanel")
  expect(panel).toHaveAttribute("tabindex", "0")
  panel.focus()
  expect(panel).toHaveFocus()
})

it("moves focus and selection between Q&A tabs with arrow, Home and End keys", async () => {
  const { client } = await loadedClient()
  const fragments = vi.fn(client.fragments)
  await renderInRouter(
    <CardQa client={{ ...client, fragments }} cardId="c:bp01-011" edition="jp" showOther={false} />,
  )
  const first = await screen.findByRole("tab", { name: "Q010" })
  const last = screen.getByRole("tab", { name: "Q011" })
  const hiddenPanels = screen.getAllByRole("tabpanel", { hidden: true })
  const lastPanel = hiddenPanels[1]
  if (!lastPanel) throw new Error("second Q&A tab panel is missing")
  await within(lastPanel).findByText(/相手の場のフェアリーは数えますか/u)
  const loadedFragmentCount = fragments.mock.calls.length
  first.focus()
  await userEvent.keyboard("{ArrowRight}")
  expect(last).toHaveFocus()
  expect(last).toHaveAttribute("aria-selected", "true")
  expect(fragments).toHaveBeenCalledTimes(loadedFragmentCount)
  await userEvent.keyboard("{End}")
  expect(last).toHaveFocus()
  await userEvent.keyboard("{Home}")
  expect(first).toHaveFocus()
  expect(first).toHaveAttribute("aria-selected", "true")
  await userEvent.keyboard("{ArrowLeft}")
  expect(last).toHaveFocus()
  expect(last).toHaveAttribute("aria-selected", "true")
})

it("only attributes visible translations when the question and answer have different sources", async () => {
  const { client } = await loadedClient(false, true)
  await renderInRouter(
    <CardQa client={client} cardId="c:bp01-002" edition="jp" showOther={false} />,
  )
  await screen.findByRole("tab", { name: "Q001" })
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

it("proves absence at the snapshot date even when today is later than the source window", async () => {
  const { client } = await loadedClient()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("synthetic snapshot did not load")
  const date = "2029-07-04"
  const covered = {
    ...snapshot,
    manifest: {
      ...snapshot.manifest,
      data_as_of: date,
      source_windows: arrayValue(snapshot.manifest["source_windows"]).map((raw) => ({
        ...objectValue(raw),
        as_of: date,
      })),
      qa_card_index: {
        jp: {
          "c:bp01-002": ["qa:bp01-002-1:v1"],
          "c:bp01-011": ["qa:bp01-011-1:v2", "qa:bp01-011-2:v1"],
        },
        en: {},
      },
    },
  }
  const fragments = vi.fn(client.fragments)
  prefsStore.set({ uiLanguage: "zh-TW", qaDisplay: "both" })
  vi.useFakeTimers({ toFake: ["Date"] })
  vi.setSystemTime(new Date("2030-01-01T00:00:00Z"))
  try {
    expect(Date.now()).toBeGreaterThan(Date.parse(date))
    await renderInRouter(
      <CardQa
        client={{ ...client, snapshot: () => covered, fragments }}
        cardId="c:bp01-001"
        edition="jp"
        showOther={false}
      />,
    )
    expect(await screen.findByText(`截至 ${date} 為止，此卡沒有官方 Q&A。`)).toBeVisible()
    expect(fragments).not.toHaveBeenCalled()
  } finally {
    vi.useRealTimers()
  }
})

it("shows the snapshot date when Q&A coverage is unknown", async () => {
  const { client } = await loadedClient()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("synthetic snapshot did not load")
  const unknown = {
    ...snapshot,
    manifest: {
      ...snapshot.manifest,
      data_as_of: "2031-07-04",
      source_windows: [],
      qa_card_ids: [],
    },
  }
  await renderInRouter(
    <CardQa
      client={{ ...client, snapshot: () => unknown }}
      cardId="c:bp01-001"
      edition="jp"
      showOther={false}
    />,
  )

  expect(await screen.findByText("截至 2031-07-04 為止，此卡沒有官方 Q&A。")).toBeVisible()
})

it("keeps the retry button when loading the Q&A list fails", async () => {
  const { client } = await loadedClient()
  const failingClient = {
    ...client,
    fragments: () => Promise.reject(new TypeError("network unavailable")),
  }
  await renderInRouter(
    <CardQa client={failingClient} cardId="c:bp01-002" edition="jp" showOther={false} />,
  )

  expect(await screen.findByRole("button", { name: "重新載入 Q&A" })).toBeVisible()
})
