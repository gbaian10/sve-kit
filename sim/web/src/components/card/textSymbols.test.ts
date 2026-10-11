import { describe, expect, it } from "vitest"

import { inlinePieces, plainText } from "./textSymbols"

const kinds = (text: string) =>
  inlinePieces(text).map((piece) => (piece.kind === "text" ? piece.text : `<${piece.alt}>`))

describe("official text symbols", () => {
  it("maps the JP alt, the zh-Hant label and the EN stem of one icon to the same file", () => {
    const [ja, zh, en] = ["{ファンファーレ}", "{入場曲}", "{icon_fanfare|[fanfare]}"].map(
      (text) => {
        const piece = inlinePieces(text)[0]
        return piece?.kind === "symbol" ? piece : undefined
      },
    )
    expect(ja?.symbol.icon).toBeTruthy()
    expect(zh?.symbol.icon).toBe(ja?.symbol.icon)
    expect(en?.symbol.icon).toBe(ja?.symbol.icon)
    expect([ja?.alt, zh?.alt, en?.alt]).toEqual(["ファンファーレ", "入場曲", "[fanfare]"])
    expect(en?.label).toBe("fanfare")
  })
  it("follows the official files where JP and EN name them differently", () => {
    const icon = (text: string) => {
      const piece = inlinePieces(text)[0]
      return piece?.kind === "symbol" ? piece.symbol.icon : undefined
    }
    expect(icon("{食事}")).toBe(icon("{icon_carrot|[feed]}"))
    expect(icon("{憑依}")).toBe(icon("{icon_ride|[ride]}"))
    expect(icon("{アドバンス起動}")).toBe(icon("{icon_adv|[adv]}"))
    // The JP site draws 起動 with icon_stand and アクト with icon_act.
    expect(icon("{起動}")).toBe(icon("{icon_stand|[engage]}"))
    expect(icon("{橫置}")).toBe(icon("{icon_act|[act]}"))
  })
  it("renders cost crystals 0–10 and X in every spelling and leaves other numbers as text", () => {
    const icon = (text: string) => {
      const piece = inlinePieces(text)[0]
      return piece?.kind === "symbol" ? piece.symbol : undefined
    }
    expect(icon("{コスト1}")?.kind).toBe("cost")
    expect(icon("{消費1}")?.icon).toBe(icon("{コスト1}")?.icon)
    expect(icon("{icon_cost01|[cost01]}")?.icon).toBe(icon("{コスト1}")?.icon)
    expect(icon("{コストX}")?.icon).toBe(icon("{COST X|[costX]}")?.icon)
    expect(icon("{消費10}")?.icon).not.toBe(icon("{消費1}")?.icon)
    expect(kinds("{コスト12}")).toEqual(["{コスト12}"])
  })
  it("keeps unknown markers and surrounding text exactly", () => {
    expect(kinds("{ATK}+1。{未知}の{進化}{コスト0}：引く。")).toEqual([
      "{ATK}+1。{未知}の",
      "<進化>",
      "<コスト0>",
      "：引く。",
    ])
    expect(kinds("")).toEqual([])
    expect(kinds("なし")).toEqual(["なし"])
  })
  it("copies text with each icon replaced by the text it stands for", () => {
    expect(plainText("{ファンファーレ}{コスト1}：1枚引く。")).toBe(
      "ファンファーレコスト1：1枚引く。",
    )
    expect(plainText("{icon_fanfare|[fanfare]} Draw.")).toBe("[fanfare] Draw.")
  })
})
