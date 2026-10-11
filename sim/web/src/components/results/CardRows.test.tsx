import { screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { SnapshotClient } from "../../data"
import { renderInRouter } from "../../test-utils"
import type { GridCell } from "./CardGrid"
import { CardList, CardTable } from "./CardRows"

const cell: GridCell = {
  key: "c:synthetic",
  to: "/cards/BP-SYNTHETIC",
  state: null,
  name: { primary: { lang: "ja", text: "Synthetic name" }, missingTranslation: false },
  summary: {
    cardId: "c:synthetic",
    printingId: "p:synthetic",
    faceId: "f:synthetic",
    cardNo: "BP-SYNTHETIC",
    region: "jp",
    classCode: null,
    name: { original: { lang: "ja", text: "Synthetic name" }, translations: {} },
    cost: 15,
    attack: 1,
    defense: 1,
  },
}

describe("row costs", () => {
  it.each(["table", "list"] as const)(
    "keeps numeric and accessible costs above 10 in %s",
    async (view) => {
      const props = { cells: [cell], onOpen: vi.fn() }
      await renderInRouter(
        view === "table" ? (
          <CardTable
            {...props}
            images={undefined}
            client={{} as SnapshotClient}
            effectOwner={() => null}
          />
        ) : (
          <CardList {...props} />
        ),
      )
      expect(screen.getByText("15")).toBeVisible()
      expect(screen.getByRole("img", { name: "費用 15" })).toBeInTheDocument()
      expect(screen.getByRole("link")).toHaveAttribute("href", "/cards/BP-SYNTHETIC")
    },
  )
})
