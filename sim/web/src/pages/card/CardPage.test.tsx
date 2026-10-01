import { screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { renderRoutes } from "../../test-utils"
import { CardPage } from "./CardPage"

const state = vi.hoisted(() => ({ available: false, current: false }))
vi.mock("../../app/snapshot", () => ({
  useCatalog: () => ({
    catalog: {
      index: {
        printingByCardNo: () => ({ card_id: "c:synthetic" }),
        facesOf: () => [{ id: "f:synthetic" }],
        wording: () => ({
          display: {
            basis: state.current
              ? "current"
              : state.available
                ? "latest_known_release"
                : "candidates",
          },
          undated_printing_ids: ["p:undated"],
        }),
        displayRevision: () => (state.available ? { name_unit_id: "t:synthetic" } : undefined),
        textUnit: () => ({ text: "合成暫顯卡名" }),
        printing: () => ({ card_no: "PR-SYNTHETIC" }),
      },
    },
  }),
}))

describe("pending wording on the card page", () => {
  it.each([false, true])(
    "shows a presentation value or loading state, available=%s",
    async (available) => {
      state.available = available
      state.current = false
      await renderRoutes([{ path: "/cards/:cardNo", Component: CardPage }], {
        initialEntries: ["/cards/BP-SYNTHETIC"],
      })
      const section = screen.getByRole("region", { name: "表記待核對" })
      expect(section).toHaveTextContent(available ? "合成暫顯卡名" : "候選載入中")
      expect(section).toHaveTextContent("日期未定版次：PR-SYNTHETIC")
      if (available) expect(section).toHaveTextContent("暫顯表記，尚未採納")
    },
  )
  it("keeps a valid current labelled as pending rather than provisional", async () => {
    state.available = true
    state.current = true
    await renderRoutes([{ path: "/cards/:cardNo", Component: CardPage }], {
      initialEntries: ["/cards/BP-SYNTHETIC"],
    })
    const section = screen.getByRole("region", { name: "表記待核對" })
    expect(section).toHaveTextContent("表記待核對")
    expect(section).not.toHaveTextContent("暫顯表記，尚未採納")
  })
})
