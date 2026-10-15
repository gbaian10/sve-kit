import { describe, expect, it } from "vitest"

import {
  CardRouteError,
  printingPath,
  resolveRoute,
  type RouteAlias,
  type RoutePrinting,
} from "./cardRoute"

const printing = (cardNo: string, changes: Partial<RoutePrinting> = {}): RoutePrinting => ({
  id: `p:${cardNo}`,
  cardId: "c:original",
  cardOwner: "set:original",
  printingOwner: "set:reprint",
  region: "jp",
  cardNo,
  cardNoState: "official",
  intId: 42,
  ...changes,
})
const official = (
  key: string,
  printings: readonly RoutePrinting[],
  aliases: readonly RouteAlias[] = [],
  overrides = new Map<string, string>(),
) => resolveRoute({ namespace: "official", key }, printings, aliases, overrides)
const alias = (
  oldKey: string,
  targetKey: string,
  namespace = "official",
  targetNamespace = "official",
): RouteAlias => ({ namespace, oldKey, targetNamespace, targetKey })

describe("permanent card routes", () => {
  it.each(["BP01-001EN", "PR-001Ⓢ", "BP01-002a"])("preserves exact %s and both owners", (no) => {
    const row = printing(no, { region: "en" })
    expect(official(no, [row])).toEqual(row)
    expect(printingPath(row)).toBe(`/cards/${encodeURIComponent(no)}`)
  })
  it("uses a confirmed variant override and rejects an unresolved collision", () => {
    const first = printing("BP01-001")
    const variant = printing("BP01-001", { id: "p:variant" })
    expect(() => official(first.cardNo, [first, variant])).toThrow(CardRouteError)
    expect(
      official(first.cardNo, [first, variant], [], new Map([[first.cardNo, variant.id]])),
    ).toBe(variant)
    expect(() =>
      official(first.cardNo, [first], [], new Map([[first.cardNo, variant.id]])),
    ).toThrow(/override/u)
  })
  it("keeps every old alias through repeated renumbering and refuses missing or cyclic targets", () => {
    const row = printing("NEW-001")
    const aliases = [alias("OLD-001", "MID-001"), alias("MID-001", "NEW-001")]
    expect(official("OLD-001", [row], aliases)).toBe(row)
    expect(() => official("OLD-001", [], aliases)).toThrow(/target/u)
    expect(() =>
      official("OLD-001", [], [alias("OLD-001", "MID-001"), alias("MID-001", "OLD-001")]),
    ).toThrow(/cyclic/u)
    expect(official(row.cardNo, [row], [alias(row.cardNo, "GONE-001")])).toBe(row)
  })
  it("allows only a unique folded key and gives exact raw spelling priority", () => {
    const first = printing("PR-001Ⓢ")
    const other = printing("PR-001S", { id: "p:other", region: "en" })
    expect(official("ｐｒ－００１Ⓢ", [first])).toBe(first)
    expect(official(first.cardNo, [first, other])).toBe(first)
    expect(() => official("pr-001s", [first, other])).toThrow(/ambiguous/u)
  })
  it("keeps provisional integers through correction without claiming guessed official numbers", () => {
    const pending = printing("GUESSED-001", { cardNoState: "provisional" })
    expect(resolveRoute({ namespace: "provisional", key: "42" }, [pending], [], new Map())).toBe(
      pending,
    )
    expect(printingPath(pending)).toBe("/cards/_provisional/42")
    expect(() => official(pending.cardNo, [pending])).toThrow(/not found/u)
    const corrected = printing("REAL-001")
    expect(
      resolveRoute(
        { namespace: "provisional", key: "42" },
        [corrected],
        [alias("42", corrected.cardNo, "provisional")],
        new Map(),
      ),
    ).toBe(corrected)
    try {
      resolveRoute({ namespace: "provisional", key: "999" }, [corrected], [], new Map())
    } catch (error) {
      expect(error).toMatchObject({ kind: "needs-update" })
      return
    }
    throw new Error("unknown integer should need an update")
  })
  it.each(["bad/key", `bad${String.fromCharCode(0)}key`, "\ud800"])(
    "rejects unsafe decoded keys",
    (key) => {
      expect(() => official(key, [])).toThrow(/invalid/u)
    },
  )
})
