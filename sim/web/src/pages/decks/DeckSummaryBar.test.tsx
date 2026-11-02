import { render, screen } from "@testing-library/react"
import { I18nextProvider } from "react-i18next"
import { beforeAll, describe, expect, it } from "vitest"

import { createDeck } from "../../domain/deck/model"
import type { DeckProblem } from "../../domain/deck/rules"
import { createI18n } from "../../i18n"
import { createDeckEditor, DeckEditorContext } from "./deck-editor"
import type { DeckFacts } from "./deck-info"
import { DeckSummaryBar } from "./DeckSummaryBar"

let i18n: Awaited<ReturnType<typeof createI18n>>
beforeAll(async () => {
  i18n = await createI18n("zh-TW")
})

const deck = createDeck(
  { name: "摘要", edition: "jp", basis: { kind: "class", classCode: "royal" } },
  new Date("2026-11-03T00:00:00.000Z"),
)
// The bar only asks whether the facts exist; the rules' result arrives as `problems`.
const loadedFacts = {} as DeckFacts
const tooFew = { severity: "error" } as DeckProblem

function renderBar(
  facts: DeckFacts | null,
  dataFailed: boolean,
  problems: readonly DeckProblem[] = [],
) {
  const editor = createDeckEditor(
    {
      deck,
      facts,
      printingsReady: facts !== null,
      dataFailed,
      retryData: () => undefined,
      restrictions: new Map(),
      problems,
      uiLanguage: "zh-TW",
      textLang: "ja",
      nameDisplay: "original",
      openCard: () => undefined,
      highlight: null,
      focusCard: () => undefined,
      pickLeader: () => undefined,
    },
    () => undefined,
  )
  render(
    <I18nextProvider i18n={i18n}>
      <DeckEditorContext value={editor}>
        <DeckSummaryBar onOpen={() => undefined} />
      </DeckEditorContext>
    </I18nextProvider>,
  )
}

describe("DeckSummaryBar", () => {
  it("does not call a deck legal before its cards have loaded", () => {
    renderBar(null, false)
    expect(
      screen.getByRole("img", { name: "資料下載中，完成後套用目前條件。" }),
    ).toBeInTheDocument()
    expect(screen.queryByRole("img", { name: "符合構築規則" })).not.toBeInTheDocument()
  })

  it("says the data failed instead of calling the deck legal", () => {
    renderBar(null, true)
    expect(screen.getByRole("img", { name: "載入失敗" })).toBeInTheDocument()
    expect(screen.queryByRole("img", { name: "符合構築規則" })).not.toBeInTheDocument()
  })

  it("counts the rules' errors once the deck is checked", () => {
    renderBar(loadedFacts, false, [tooFew, tooFew])
    expect(screen.getByText("問題 2 個")).toBeInTheDocument()
  })

  it("marks a checked deck without errors as legal", () => {
    renderBar(loadedFacts, false, [])
    expect(screen.getByRole("img", { name: "符合構築規則" })).toBeInTheDocument()
  })
})
