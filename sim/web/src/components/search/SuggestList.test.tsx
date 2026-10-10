import { screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
import { SuggestList, type SuggestRow } from "./SuggestList"

const row: SuggestRow = {
  summary: {
    cardId: "c:synthetic",
    printingId: "p:synthetic",
    faceId: "f:synthetic",
    cardNo: "SYN-001",
    region: "jp",
    classCode: null,
    name: { original: { lang: "ja", text: "Synthetic original" }, translations: {} },
    cost: null,
    attack: null,
    defense: null,
  },
  name: {
    primary: { lang: "zh-Hant", text: "Synthetic translation" },
    secondary: { lang: "ja", text: "Synthetic original" },
    missingTranslation: false,
    lowConfidence: true,
    jpSource: true,
  },
}

describe("current translation notices in suggestions", () => {
  it.each([
    ["zh-TW", "待校對", "依日文原文翻譯"],
    ["ja", "翻訳要校正", "日本語原文からの訳"],
    ["en", "Translation awaiting proofreading", "Translated from Japanese source"],
  ] as const)(
    "keeps the quality and source notices visible in %s",
    async (language, quality, source) => {
      await renderInRouter(
        <SuggestList
          id="synthetic-list"
          rows={[row]}
          activeIndex={0}
          images={undefined}
          onPick={vi.fn()}
          onHover={vi.fn()}
        />,
        { language },
      )
      expect(screen.getByText(quality)).toBeVisible()
      expect(screen.getByText(source)).toBeVisible()
      expect(screen.getByRole("option")).toHaveAccessibleName(expect.stringContaining(quality))
      expect(screen.getByRole("option")).toHaveAccessibleName(
        expect.stringContaining("Synthetic original"),
      )
      expect(screen.getByText(source).closest(".truncate")).toBeNull()
    },
  )
})
