import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it, vi } from "vitest"

import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import type { TextLang } from "../../domain/search"
import { renderInRouter } from "../../test-utils"
import { ConditionChips } from "./ConditionChips"
import { vocabularyOptionLabel } from "./facet-labels"
import { KeywordFilter } from "./KeywordFilter"

const keywords = [
  {
    id: "kw:ward",
    code: "ward",
    name: { original: { lang: "ja" as const, text: "守護" }, translations: { "zh-Hant": "守護" } },
  },
  {
    id: "kw:storm",
    code: "storm",
    name: { original: { lang: "ja" as const, text: "疾走" }, translations: { "zh-Hant": "疾馳" } },
  },
]
function Harness({ textLang = "zh-Hant" }: { readonly textLang?: TextLang }) {
  const [query, setQuery] = useState(DEFAULT_QUERY)
  return (
    <>
      <KeywordFilter
        options={{ keywords, coverage: "partial" }}
        edition="jp"
        textLang={textLang}
        selected={query.mechanics}
        onChange={(mechanics) => {
          setQuery(parseQuery(formatQuery({ ...query, mechanics })))
        }}
      />
      <ConditionChips
        query={query}
        edition="jp"
        groups={{}}
        classLabel={(code) => code}
        rarityLabel={(code) => code}
        typeLabel={(code) => code}
        traitLabel={(code) => code}
        artistLabel={(id) => id}
        setLabel={(code) => code}
        keywordLabel={(id) =>
          vocabularyOptionLabel(
            keywords.find((keyword) => keyword.id === id)?.name ?? {
              original: { lang: "ja", text: id },
              translations: {},
            },
            textLang,
          )
        }
        onChange={setQuery}
        onClear={() => {
          setQuery(DEFAULT_QUERY)
        }}
      />
      <output aria-label="url">{formatQuery(query).toString()}</output>
    </>
  )
}
it("toggles multiple has chips, round trips the URL and removes conditions independently", async () => {
  await renderInRouter(<Harness />)
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字/u }))
  expect(screen.getByText(/標註尚未完整/u)).toBeVisible()
  await userEvent.click(screen.getByRole("button", { name: "守護" }))
  await userEvent.click(screen.getByRole("button", { name: "疾馳" }))
  expect(decodeURIComponent(screen.getByLabelText("url").textContent)).toBe("mech=kw:storm,kw:ward")
  expect(screen.getByRole("button", { name: "守護" })).toHaveAttribute("aria-pressed", "true")
  await userEvent.click(screen.getByRole("button", { name: "移除條件：關鍵字：守護" }))
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字:/u }))
  expect(screen.getByRole("button", { name: "守護" })).toHaveAttribute("aria-pressed", "false")
  expect(decodeURIComponent(screen.getByLabelText("url").textContent)).toBe("mech=kw:storm")
  await userEvent.click(screen.getByRole("button", { name: "清除關鍵字" }))
  expect(screen.getByLabelText("url")).toBeEmptyDOMElement()
})
it.each([
  ["ja", "ja", "疾走"],
  ["en", "en", "疾走"],
  ["zh-TW", "zh-Hant", "疾馳"],
] as const)("uses the vocabulary fallback in %s", async (language, textLang, label) => {
  await renderInRouter(<Harness textLang={textLang} />, { language })
  await userEvent.click(screen.getByRole("button", { name: /^(?:關鍵字|Keywords|キーワード)/u }))
  expect(screen.getByRole("button", { name: label })).toBeVisible()
})
it("disables EN when the edition's coverage is unknown, even with a selected URL condition", async () => {
  await renderInRouter(
    <KeywordFilter
      options={{ keywords, coverage: "unknown" }}
      edition="en"
      textLang="en"
      selected={{ "kw:ward": "has" }}
      onChange={() => undefined}
    />,
    { language: "en" },
  )
  await userEvent.click(screen.getByRole("button", { name: /^Keywords/u }))
  const group = screen.getByRole("group", { name: "Keywords" })
  expect(within(group).getByText("English edition is not tagged yet")).toBeVisible()
  expect(within(group).getByRole("button", { name: "守護" })).toBeDisabled()
  expect(within(group).getByRole("button", { name: "疾走" })).toBeDisabled()
})

it("keeps keyword chips in place and disabled until tagging finishes", async () => {
  const onChange = vi.fn()
  const props = { edition: "jp" as const, textLang: "zh-Hant" as const, selected: {}, onChange }
  function LoadingHarness() {
    const [coverage, setCoverage] = useState<"pending" | "partial">("pending")
    return (
      <>
        <KeywordFilter {...props} options={{ keywords, coverage }} />
        <button
          type="button"
          onClick={() => {
            setCoverage("partial")
          }}
        >
          完成標註
        </button>
      </>
    )
  }
  await renderInRouter(<LoadingHarness />)
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字/u }))
  const ward = screen.getByRole("button", { name: "守護" })
  const storm = screen.getByRole("button", { name: "疾馳" })
  expect(ward).toBeDisabled()
  expect(storm).toBeDisabled()
  expect(screen.getByRole("status")).toHaveTextContent("標註載入中")
  await userEvent.click(ward)
  expect(onChange).not.toHaveBeenCalled()
  await userEvent.click(screen.getByRole("button", { name: "完成標註" }))
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字:/u }))
  expect(screen.getByRole("button", { name: "守護" })).toBe(ward)
  expect(screen.getByRole("button", { name: "疾馳" })).toBe(storm)
  expect(ward).toBeEnabled()
  expect(storm).toBeEnabled()
  await userEvent.click(ward)
  expect(onChange).toHaveBeenCalledWith({ "kw:ward": "has" })
})
