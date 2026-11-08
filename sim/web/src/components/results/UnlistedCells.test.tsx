import { act, screen, within } from "@testing-library/react"
import { afterEach, beforeEach, expect, it, vi } from "vitest"

import { useAbilityPreview } from "../../app/snapshot"
import { DEFAULT_PREFS, prefsStore } from "../../settings"
import { renderInRouter } from "../../test-utils"
import { CardGrid, type GridCell } from "./CardGrid"
import { CardList, CardTable } from "./CardRows"

vi.mock("../../app/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../app/snapshot")>()),
  useAbilityPreview: vi.fn(() => ({ phase: "pending" })),
}))

beforeEach(() => {
  vi.mocked(useAbilityPreview).mockReset().mockReturnValue({ phase: "pending" })
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
})
afterEach(() => {
  act(() => {
    prefsStore.set(DEFAULT_PREFS)
  })
  vi.unstubAllGlobals()
})

const cell = (key: string, printedCode: string | null, unlisted = true): GridCell => ({
  key,
  to: `/cards/_provisional/${key}`,
  state: null,
  name: { primary: { lang: "ja", text: `名-${key}` }, missingTranslation: false },
  summary: {
    cardId: `c:${key}`,
    printingId: `p:${key}`,
    faceId: `f:${key}`,
    cardNo: unlisted ? "" : "BP01-001",
    cardNoState: unlisted ? "none" : "official",
    intId: 55001,
    ...(unlisted ? { unlisted: true, printedCode } : {}),
    region: "jp",
    classCode: null,
    name: { original: { lang: "ja", text: `名-${key}` }, translations: {} },
    cost: null,
    attack: null,
    defense: null,
  },
})
const coded = cell("coded", "2024千葉-A")
const bare = cell("bare", null)
const listed = cell("listed", null, false)
const typeLabel = (code: string) => code
const classLabel = () => "中立"

it.each([
  ["zh-TW", "官方資料庫未收錄"],
  ["ja", "公式データベース未収録"],
  ["en", "Not in official card database"],
] as const)("tags an unlisted card under its name in the grid in %s", async (language, badge) => {
  await renderInRouter(
    <CardGrid cells={[coded, bare, listed]} images={undefined} onOpen={vi.fn()} />,
    { language },
  )
  const links = screen.getAllByRole("link")
  expect(links).toHaveLength(3)
  const [first, second, third] = links as [HTMLElement, HTMLElement, HTMLElement]
  expect(within(first).getByText(badge)).toBeVisible()
  expect(within(second).getByText(badge)).toBeVisible()
  expect(within(third).queryByText(badge)).not.toBeInTheDocument()
  // The number slot holds the printed code, or nothing; never the empty official number.
  expect(within(first).getByText("2024千葉-A")).toBeVisible()
  expect(within(third).getByText("BP01-001")).toBeVisible()
  expect(within(second).getByText("名-bare").nextElementSibling).toHaveTextContent(/^$/u)
})

it("explains the tag on hover", async () => {
  await renderInRouter(<CardGrid cells={[coded]} images={undefined} onOpen={vi.fn()} />)
  expect(screen.getByText("官方資料庫未收錄").parentElement).toHaveAttribute(
    "title",
    "官方卡表沒有收錄這張卡；卡名與卡圖依實卡或官方宣傳整理",
  )
})

it("tags an unlisted card in the table view and asks for no card text", async () => {
  await renderInRouter(
    <CardTable
      cells={[coded, listed]}
      images={undefined}
      artOf={() => undefined}
      typeLabel={typeLabel}
      onOpen={vi.fn()}
    />,
  )
  const rows = screen.getAllByRole("listitem")
  expect(within(rows[0] ?? document.body).getByText("官方資料庫未收錄")).toBeVisible()
  expect(within(rows[0] ?? document.body).getByText("2024千葉-A")).toBeVisible()
  expect(within(rows[1] ?? document.body).queryByText("官方資料庫未收錄")).toBeNull()
  expect(rows[0]?.querySelector("[data-preview]")).toBeNull()
  expect(rows[1]?.querySelector("[data-preview]")).not.toBeNull()
})

it("tags an unlisted card in the spreadsheet list and asks for no card text", async () => {
  vi.stubGlobal("IntersectionObserver", undefined)
  await renderInRouter(
    <CardList
      typeLabel={typeLabel}
      classLabel={classLabel}
      cells={[coded, bare, listed]}
      onOpen={vi.fn()}
    />,
  )
  const rows = screen.getAllByRole("row").slice(1)
  const [first, second, third] = rows as [HTMLElement, HTMLElement, HTMLElement]
  expect(within(first).getByText("官方資料庫未收錄")).toBeVisible()
  expect(first.querySelector("[data-list-number]")).toHaveTextContent("2024千葉-A")
  expect(second.querySelector("[data-list-number]")).toHaveTextContent(/^$/u)
  expect(within(second).getByText("官方資料庫未收錄")).toBeVisible()
  expect(third.querySelector("[data-list-number]")).toHaveTextContent("BP01-001")
  expect(within(third).queryByText("官方資料庫未收錄")).toBeNull()
  expect(first.querySelector("[data-list-effect-text]")).toBeEmptyDOMElement()
  const asked = vi.mocked(useAbilityPreview).mock.calls.map(([id]) => id)
  expect(asked).toContain("p:listed")
  expect(asked).not.toContain("p:coded")
  expect(asked).not.toContain("p:bare")
})
