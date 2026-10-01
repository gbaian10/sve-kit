import { describe, expect, it } from "vitest"

import { cardTextToPlain, type CardTextVocabulary, parseCardText } from "./cardText"

const vocabulary: CardTextVocabulary = {
  spellings: [
    {
      symbolId: "sym:fanfare",
      code: "fanfare",
      lang: "ja",
      prefix: "ファンファーレ",
      suffix: "",
      parse: "literal",
      variables: [],
    },
    {
      symbolId: "sym:cost",
      code: "cost",
      lang: "ja",
      prefix: "コスト",
      suffix: "",
      parse: "uint",
      variables: [],
      minimum: 0,
      maximum: 10,
    },
    {
      symbolId: "sym:cost",
      code: "cost",
      lang: "ja",
      prefix: "コスト",
      suffix: "",
      parse: "variable",
      variables: ["X"],
    },
    {
      symbolId: "sym:fanfare",
      code: "fanfare",
      lang: "en",
      prefix: "[fanfare]",
      suffix: "",
      parse: "literal",
      variables: [],
    },
  ],
  keywords: [
    { keywordId: "kw:ward", lang: "ja", name: "守護" },
    { keywordId: "kw:ward", lang: "en", name: "Ward" },
  ],
}

describe("parseCardText", () => {
  it("splits symbols, parameters, keywords and breaks", () => {
    expect(
      parseCardText("{ファンファーレ}{コスト2}のカードを1枚引く。\n【守護】", "ja", vocabulary),
    ).toEqual([
      { kind: "symbol", code: "fanfare", symbolId: "sym:fanfare", raw: "ファンファーレ" },
      { kind: "symbol", code: "cost", symbolId: "sym:cost", parameter: "2", raw: "コスト2" },
      { kind: "text", text: "のカードを1枚引く。" },
      { kind: "break" },
      { kind: "keyword", keywordId: "kw:ward", name: "守護" },
    ])
  })

  it("parses variable parameters, keyword parameters and leading zeros", () => {
    expect(parseCardText("{コストX}{コスト02}", "ja", vocabulary)).toEqual([
      { kind: "symbol", code: "cost", symbolId: "sym:cost", parameter: "X", raw: "コストX" },
      { kind: "symbol", code: "cost", symbolId: "sym:cost", parameter: "2", raw: "コスト02" },
    ])
    expect(parseCardText("【守護_2】", "ja", vocabulary)).toEqual([
      { kind: "keyword", keywordId: "kw:ward", name: "守護", parameter: "2" },
    ])
  })

  it("keeps out-of-range and unsafe numbers as raw tokens", () => {
    expect(
      parseCardText("{コスト10}{コスト11}{コスト99999999999999999999}", "ja", vocabulary),
    ).toEqual([
      { kind: "symbol", code: "cost", symbolId: "sym:cost", parameter: "10", raw: "コスト10" },
      { kind: "unknown", raw: "{コスト11}" },
      { kind: "unknown", raw: "{コスト99999999999999999999}" },
    ])
  })

  it("keeps unknown tokens and wrong-language spellings visible", () => {
    expect(parseCardText("{謎}【謎】{ファンファーレ}", "ja", vocabulary)).toEqual([
      { kind: "unknown", raw: "{謎}" },
      { kind: "unknown", raw: "【謎】" },
      { kind: "symbol", code: "fanfare", symbolId: "sym:fanfare", raw: "ファンファーレ" },
    ])
    expect(parseCardText("{ファンファーレ}", "en", vocabulary)).toEqual([
      { kind: "unknown", raw: "{ファンファーレ}" },
    ])
    expect(parseCardText("", "ja", vocabulary)).toEqual([])
    expect(parseCardText("plain", "ja", vocabulary)).toEqual([{ kind: "text", text: "plain" }])
  })

  it("renders plain copy text from segments", () => {
    const segments = parseCardText("{コスト2}{謎}【守護_2】\nend", "ja", vocabulary)
    expect(
      cardTextToPlain(segments, (id) => (id === "sym:cost" ? "コスト{amount}" : undefined)),
    ).toBe("コスト2{謎}【守護_2】\nend")
  })

  it("replaces copy-pattern placeholders whatever the parameter is called", () => {
    const segments = parseCardText("{コスト2}", "ja", vocabulary)
    expect(cardTextToPlain(segments, () => "[cost{x-value2}]")).toBe("[cost2]")
    expect(cardTextToPlain(segments, () => "コスト{amount_1}")).toBe("コスト2")
  })
})
