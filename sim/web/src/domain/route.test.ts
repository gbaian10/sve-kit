import { describe, expect, it } from "vitest"

import { cardPath, resolveCardRoute, resolveProvisionalRoute, type RouteLookups } from "./route"

const lookups: RouteLookups = {
  printingByCardNo: (cardNo) =>
    ({ "BP01-001": "p:bp01-001", "BP01-002a": "p:bp01-002-alt", "BP01-002": "p:bp01-002" })[cardNo],
  printingByIntId: (intId) => (intId === 9001 ? "p:prov-9001" : undefined),
  alias: (namespace, key) =>
    namespace === "official" && key === "BP01-002A"
      ? { namespace: "official", key: "BP01-002a" }
      : namespace === "provisional" && key === "9000"
        ? { namespace: "official", key: "BP01-001" }
        : undefined,
  override: (routeKey) => (routeKey === "BP01-002" ? "p:bp01-002-override" : undefined),
  nameOf: (printingId) =>
    ({ "p:bp01-001": "試作の見習い兵", "p:bp01-002-override": "試作の隊長" })[printingId],
}

describe("card routes", () => {
  it("finds a printing and demands the canonical slug", () => {
    expect(resolveCardRoute("BP01-001", "試作の見習い兵", lookups)).toEqual({
      kind: "found",
      printingId: "p:bp01-001",
      canonicalPath:
        "/cards/BP01-001/%E8%A9%A6%E4%BD%9C%E3%81%AE%E8%A6%8B%E7%BF%92%E3%81%84%E5%85%B5",
    })
    expect(resolveCardRoute("BP01-001", undefined, lookups)).toEqual({
      kind: "redirect",
      to: cardPath("BP01-001", "試作の見習い兵"),
    })
    expect(resolveCardRoute("BP01-001", "wrong", lookups)).toEqual({
      kind: "redirect",
      to: cardPath("BP01-001", "試作の見習い兵"),
    })
  })

  it("keeps a slug-less path when the printing has no name to add", () => {
    expect(resolveCardRoute("BP01-002a", undefined, lookups)).toEqual({
      kind: "found",
      printingId: "p:bp01-002-alt",
      canonicalPath: "/cards/BP01-002a",
    })
    expect(resolveCardRoute("BP01-002a", "x", lookups)).toEqual({
      kind: "redirect",
      to: "/cards/BP01-002a",
    })
  })

  it("applies aliases as redirects and overrides as the opened printing", () => {
    expect(resolveCardRoute("BP01-002A", undefined, lookups)).toEqual({
      kind: "redirect",
      to: "/cards/BP01-002a",
    })
    expect(resolveCardRoute("BP01-002", "試作の隊長", lookups)).toMatchObject({
      kind: "found",
      printingId: "p:bp01-002-override",
    })
    expect(resolveCardRoute("BP99-999", undefined, lookups)).toEqual({ kind: "missing" })
  })

  it("resolves provisional ids and their aliases", () => {
    expect(resolveProvisionalRoute("9001", lookups)).toEqual({
      kind: "found",
      printingId: "p:prov-9001",
      canonicalPath: "/cards/_provisional/9001",
    })
    expect(resolveProvisionalRoute("9000", lookups)).toEqual({
      kind: "redirect",
      to: "/cards/BP01-001",
    })
    expect(resolveProvisionalRoute("nope", lookups)).toEqual({ kind: "missing" })
    expect(resolveProvisionalRoute("1", lookups)).toEqual({ kind: "missing" })
  })
})
