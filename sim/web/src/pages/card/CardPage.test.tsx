import { screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { renderRoutes } from "../../test-utils"
import { CardPage } from "./CardPage"

const state = vi.hoisted(() => ({
  available: false,
  current: false,
  crossRegion: false,
  wording: vi.fn(),
}))
vi.mock("../../app/snapshot", () => {
  const printing = () => ({
    card_id: "c:synthetic",
    region: state.crossRegion ? "en" : "jp",
    int_id: 42,
  })
  return {
    useCatalog: () => ({
      catalog: {
        index: {
          cards: [{ id: "c:synthetic" }],
          printingsOf: () => [printing()],
          printingByCardNo: (region: string) =>
            state.crossRegion && region === "jp" ? undefined : printing(),
          facesOf: () => [{ id: "f:synthetic" }],
          wording: (face: string, region: string) => {
            state.wording(face, region)
            return {
              display: {
                basis: state.current
                  ? "current"
                  : state.available
                    ? "latest_known_release"
                    : "candidates",
              },
              undated_printing_ids: ["p:undated"],
            }
          },
          displayRevision: () => (state.available ? { name_unit_id: "t:synthetic" } : undefined),
          textUnit: () => ({ text: "Synthetic presentation name" }),
          printing: () => ({ card_no: "PR-SYNTHETIC" }),
        },
      },
    }),
  }
})

beforeEach(() => {
  state.available = false
  state.current = false
  state.crossRegion = false
  state.wording.mockClear()
})

describe("pending wording on the card page", () => {
  it.each([
    ["zh-TW", "表記未定", "依已知發售日暫顯"],
    ["ja", "表記未定", "既知の発売日に基づく仮表示"],
    ["en", "Wording pending", "Provisional display by known release date"],
  ] as const)(
    "shows visible pending and release-day labels in %s",
    async (language, pending, provisional) => {
      state.available = true
      await renderRoutes([{ path: "/cards/:cardNo", Component: CardPage }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
        language,
      })
      const section = screen.getByRole("region", { name: pending })
      expect(screen.getByRole("heading", { name: pending })).toBeVisible()
      expect(section).toHaveTextContent("Synthetic presentation name")
      expect(screen.getByText(new RegExp(provisional))).toBeVisible()
      expect(screen.getByText("PR-SYNTHETIC")).toHaveClass("whitespace-nowrap")
    },
  )
  it.each([
    ["zh-TW", "有多個候選，尚未選定"],
    ["ja", "複数の候補があり、まだ選定されていません"],
    ["en", "Multiple candidates; none selected yet"],
  ] as const)("shows one honest unselected message in %s", async (language, unselected) => {
    await renderRoutes([{ path: "/cards/:cardNo", Component: CardPage }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
      language,
    })
    expect(screen.getAllByText(unselected)).toHaveLength(1)
    expect(
      screen.queryByText(/候選載入中|候補を読み込み中|Loading candidates/),
    ).not.toBeInTheDocument()
  })
  it("keeps a valid current labelled as pending rather than provisional", async () => {
    state.available = true
    state.current = true
    await renderRoutes([{ path: "/cards/:cardNo", Component: CardPage }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    expect(screen.getByRole("heading", { name: "表記未定" })).toBeVisible()
    expect(screen.queryByText(/依已知發售日暫顯/)).not.toBeInTheDocument()
  })
  it.each(["/cards/BP-SYNTHETIC", "/p/42"])(
    "uses the resolved printing's region for %s",
    async (path) => {
      state.crossRegion = true
      state.available = true
      await renderRoutes(
        [
          { path: "/cards/:cardNo", Component: CardPage },
          { path: "/p/:intId", Component: CardPage },
        ],
        { initialEntries: [path] },
      )
      expect(state.wording).toHaveBeenCalledWith("f:synthetic", "en")
      expect(screen.getByText("Synthetic presentation name")).toBeVisible()
    },
  )
})
