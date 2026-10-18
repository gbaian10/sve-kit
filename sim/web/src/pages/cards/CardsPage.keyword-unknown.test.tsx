import { screen } from "@testing-library/react"
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { CARDS } from "../../../scripts/fixture/cards"
import { routes } from "../../app/routes"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderRoutes } from "../../test-utils"
import { TestSearchWorker } from "../../test-utils/search-worker"

// A separate root lifetime keeps this alternate snapshot isolated from the default page fixture.
const built = await buildSnapshot({
  encodeImage: () => Promise.resolve(new Uint8Array()),
  cards: CARDS.map((card) => ({ ...card, enBlock: ["unconfirmed_mapping"] })),
})

beforeAll(() => {
  vi.stubGlobal("Worker", TestSearchWorker)
  vi.stubGlobal("fetch", (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    const bytes = built.files.get(url.replace(/^.*\/cdn\//u, ""))
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
})
afterAll(() => {
  prefsStore.set(DEFAULT_PREFS)
  vi.unstubAllGlobals()
})

describe("filter panel", () => {
  it.each([
    ["zh-TW", "這些條件還不能套用：關鍵字"],
    ["ja", "まだ適用できない条件：キーワード"],
    ["en", "These conditions cannot be applied yet: Keywords"],
  ] as const)(
    "explains unsupported English keyword links in %s with the panel collapsed",
    async (language, notice) => {
      prefsStore.set({ uiLanguage: language, filterPanelOpen: false })
      await renderRoutes(routes, { initialEntries: ["/cards?ed=en&mech=kw%3Award"], language })
      expect(await screen.findByRole("alert")).toHaveTextContent(notice)
      expect(screen.queryByText("0 個版次")).not.toBeInTheDocument()
    },
  )
})
