import { act, render, screen } from "@testing-library/react"
import type { ReactNode } from "react"
import { I18nextProvider } from "react-i18next"
import { beforeAll, describe, expect, it, vi } from "vitest"

import { createI18n } from "../../i18n"
import type { DeckImageInput } from "./deck-image"
import { DeckPicture } from "./DeckPicture"

// Each drawing finishes when the test says so, as a slow card download would.
const drawings: { title: string; finish: () => void }[] = []
vi.mock("./deck-image", () => ({
  renderDeckImage: (input: DeckImageInput) =>
    new Promise<HTMLCanvasElement>((resolve) => {
      const canvas = document.createElement("canvas")
      canvas.dataset["title"] = input.title
      drawings.push({
        title: input.title,
        finish: () => {
          resolve(canvas)
        },
      })
    }),
}))

const deck = (title: string, cards: string[]): DeckImageInput => ({
  size: "small",
  title,
  subtitle: "",
  leader: null,
  leaderLine: "",
  sections: [{ label: "Main", cards: cards.map((label) => ({ src: label, label, count: 2 })) }],
  footer: "",
  missing: "placeholder",
  colors: { page: "", text: "", muted: "", badge: "", badgeText: "" },
})

let i18n: Awaited<ReturnType<typeof createI18n>>
beforeAll(async () => {
  i18n = await createI18n("en")
})
const wrapper = ({ children }: { children: ReactNode }) => (
  <I18nextProvider i18n={i18n}>{children}</I18nextProvider>
)

async function finish(title: string) {
  await act(async () => {
    drawings.find((drawing) => drawing.title === title)?.finish()
    await Promise.resolve()
  })
}

describe("DeckPicture", () => {
  it("never shows a picture that does not match the current input", async () => {
    const a = deck("A", ["Alpha"])
    const { rerender } = render(<DeckPicture input={a} label="A picture" />, { wrapper })
    await finish("A")
    expect(screen.getByRole("img", { name: "A picture" })).toBeVisible()
    // The same content in a new object is not drawn again.
    rerender(<DeckPicture input={deck("A", ["Alpha"])} label="A picture" />)
    expect(drawings.filter((drawing) => drawing.title === "A")).toHaveLength(1)

    // B's rows arrive later than its text: A stays out of sight meanwhile, not shown under B's name.
    rerender(<DeckPicture input={null} label="B picture" />)
    expect(screen.queryByRole("img")).toBeNull()
    expect(screen.getByRole("status")).toHaveTextContent("Loading card images")
    const b = deck("B", ["Beta"])
    rerender(<DeckPicture input={b} label="B picture" />)
    expect(screen.queryByRole("img")).toBeNull()
    // Screen readers can already check B's cards.
    expect(screen.getByText("Beta ×2")).toBeInTheDocument()
    await finish("B")
    const shown = screen.getByRole("img", { name: "B picture" })
    expect(shown.querySelector("canvas")?.dataset["title"]).toBe("B")
    expect(screen.getByRole("status")).toHaveTextContent("")
  })
})
