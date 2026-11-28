import { screen, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { expect, it, vi } from "vitest"

import { perRelation } from "../../domain/mechanics"
import { formatQuery, parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import type { TextLang } from "../../domain/search"
import { renderInRouter } from "../../test-utils"
import { ConditionChips } from "./ConditionChips"
import { vocabularyOptionLabel } from "./facet-labels"
import { KeywordFilter } from "./KeywordFilter"

const keywords = [
  {
    id: "kw:assail",
    code: "assail",
    name: {
      original: { lang: "ja" as const, text: "指定攻擊" },
      translations: { "zh-Hant": "指定攻擊" },
    },
  },
  {
    id: "kw:intimidate",
    code: "intimidate",
    name: { original: { lang: "ja" as const, text: "威懾" }, translations: { "zh-Hant": "威懾" } },
  },
  {
    id: "kw:aura",
    code: "aura",
    name: { original: { lang: "ja" as const, text: "光紋" }, translations: { "zh-Hant": "光紋" } },
  },
  {
    id: "kw:ward",
    code: "ward",
    name: { original: { lang: "ja" as const, text: "守護" }, translations: { "zh-Hant": "守護" } },
  },
  {
    id: "kw:bane",
    code: "bane",
    name: { original: { lang: "ja" as const, text: "必殺" }, translations: { "zh-Hant": "必殺" } },
  },
  {
    id: "kw:drain",
    code: "drain",
    name: { original: { lang: "ja" as const, text: "吸血" }, translations: { "zh-Hant": "吸血" } },
  },
  {
    id: "kw:rush",
    code: "rush",
    name: { original: { lang: "ja" as const, text: "突進" }, translations: { "zh-Hant": "突進" } },
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
        options={{
          keywords,
          coverage: perRelation("partial"),
          unknownCards: perRelation(5),
        }}
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
it("toggles keywords, round trips the URL and removes conditions independently", async () => {
  await renderInRouter(<Harness />)
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字/u }))
  expect(screen.getByText("仍有 5 張卡的「自身」尚無法判定")).toBeVisible()
  await userEvent.click(screen.getByRole("button", { name: "守護" }))
  await userEvent.click(screen.getByRole("button", { name: "疾馳" }))
  expect(decodeURIComponent(screen.getByLabelText("url").textContent)).toBe("mech=kw:storm,kw:ward")
  expect(screen.getByRole("button", { name: "守護" })).toHaveAttribute("aria-pressed", "true")
  await userEvent.click(screen.getByRole("button", { name: "移除條件：關鍵字：守護" }))
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字:/u }))
  expect(screen.getByRole("button", { name: "守護" })).toHaveAttribute("aria-pressed", "false")
  expect(decodeURIComponent(screen.getByLabelText("url").textContent)).toBe("mech=kw:storm")
  // A second press clears the condition; there is no excluded state.
  await userEvent.click(screen.getByRole("button", { name: "疾馳" }))
  expect(screen.getByLabelText("url")).toBeEmptyDOMElement()
})
it("keeps held and granted keywords on separate tabs and names the relation in chips and the URL", async () => {
  await renderInRouter(<Harness />)
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字/u }))
  await userEvent.click(screen.getByRole("button", { name: "守護" }))
  const tabs = screen.getByRole("tablist", { name: "關鍵字的關係" })
  expect(within(tabs).getAllByRole("tab")).toHaveLength(2)
  within(tabs).getByRole("tab", { name: /^自身/u }).focus()
  await userEvent.keyboard("{ArrowRight}")
  expect(within(tabs).getByRole("tab", { name: "賦予" })).toHaveAttribute("aria-selected", "true")
  // The same keyword is a separate condition under the other relation.
  expect(screen.getByRole("button", { name: "守護" })).toHaveAttribute("aria-pressed", "false")
  await userEvent.click(screen.getByRole("button", { name: "突進" }))
  await userEvent.click(screen.getByRole("button", { name: "守護" }))
  expect(decodeURIComponent(screen.getByLabelText("url").textContent)).toBe(
    "mech=kw:rush.grants,kw:ward,kw:ward.grants",
  )
  expect(within(tabs).getByRole("tab", { name: /^自身\s*已選 1 項$/u })).toBeVisible()
  expect(within(tabs).getByRole("tab", { name: /^賦予\s*已選 2 項$/u })).toBeVisible()
  await userEvent.click(screen.getByRole("button", { name: "移除條件：賦予：突進" }))
  expect(decodeURIComponent(screen.getByLabelText("url").textContent)).toBe(
    "mech=kw:ward,kw:ward.grants",
  )
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字:/u }))
  await userEvent.click(screen.getByRole("button", { name: "清除關鍵字" }))
  expect(screen.getByLabelText("url")).toBeEmptyDOMElement()
})
it("shows the eight keyword chips in the requested order with their icons", async () => {
  await renderInRouter(<Harness />)
  await userEvent.click(screen.getByRole("button", { name: /^關鍵字/u }))
  const group = screen.getByRole("group", { name: "關鍵字" })
  expect(
    within(group)
      .getAllByRole("button")
      .map((button) => button.textContent),
  ).toEqual(["指定攻擊", "威懾", "光紋", "守護", "必殺", "吸血", "突進", "疾馳"])
  expect(group.querySelectorAll("img")).toHaveLength(6)
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
it("disables only the relations an edition has no tags for and still lets conditions be cleared", async () => {
  const onChange = vi.fn()
  await renderInRouter(
    <KeywordFilter
      options={{
        keywords,
        coverage: { has: "complete", grants: "unknown" },
        unknownCards: { has: 0, grants: 2 },
      }}
      edition="en"
      textLang="en"
      selected={["kw:ward.grants"]}
      onChange={onChange}
    />,
    { language: "en" },
  )
  await userEvent.click(screen.getByRole("button", { name: /^Keywords/u }))
  const group = screen.getByRole("group", { name: "Keywords" })
  expect(within(group).getByRole("button", { name: "守護" })).toBeEnabled()
  await userEvent.click(within(group).getByRole("tab", { name: /^Gives keyword/u }))
  expect(within(group).getByRole("status")).toHaveTextContent("English edition is not tagged yet")
  expect(within(group).getByRole("button", { name: "守護" })).toBeDisabled()
  expect(within(group).getByRole("button", { name: "疾走" })).toBeDisabled()
  await userEvent.click(screen.getByRole("button", { name: "Clear Keywords" }))
  expect(onChange).toHaveBeenCalledWith([])
})

it("keeps keyword chips in place and disabled until tagging finishes", async () => {
  const onChange = vi.fn()
  const props = { edition: "jp" as const, textLang: "zh-Hant" as const, selected: [], onChange }
  function LoadingHarness() {
    const [coverage, setCoverage] = useState<"pending" | "partial">("pending")
    return (
      <>
        <KeywordFilter {...props} options={{ keywords, coverage: perRelation(coverage) }} />
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
  expect(onChange).toHaveBeenCalledWith(["kw:ward"])
})

it("shows a concrete unknown-card count and removes it when self coverage is complete", async () => {
  function CountHarness() {
    const [unknownCards, setUnknown] = useState(12)
    return (
      <>
        <KeywordFilter
          options={{
            keywords,
            coverage: perRelation(unknownCards ? "partial" : "complete"),
            unknownCards: perRelation(unknownCards),
          }}
          edition="en"
          textLang="en"
          selected={[]}
          onChange={() => undefined}
        />
        <button
          type="button"
          onClick={() => {
            setUnknown(0)
          }}
        >
          resolve
        </button>
      </>
    )
  }
  await renderInRouter(<CountHarness />, { language: "en" })
  await userEvent.click(screen.getByRole("button", { name: /^Keywords/u }))
  expect(screen.getByRole("status")).toHaveTextContent("undetermined for 12 cards")
  expect(screen.getByRole("button", { name: "守護" })).toBeEnabled()
  await userEvent.click(screen.getByRole("button", { name: "resolve" }))
  await userEvent.click(screen.getByRole("button", { name: /^Keywords/u }))
  expect(screen.getByRole("status")).toBeEmptyDOMElement()
})
