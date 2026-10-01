// Card URLs (architecture §2, snapshot-format §8): `/cards/{card_no}[/{slug}]` for official numbers,
// `/cards/_provisional/{int_id}` for provisional ones. The number decides the card; the slug is the
// original name and only makes the link readable, so a wrong or missing slug is replaced with the
// canonical one. `card_route_alias` rows are permanent redirects, `route_override` picks the
// printing a shared number opens as.
export type RouteNamespace = "official" | "provisional"

export interface RouteLookups {
  readonly printingByCardNo: (cardNo: string) => string | undefined
  readonly printingByIntId: (intId: number) => string | undefined
  readonly alias: (
    namespace: RouteNamespace,
    key: string,
  ) => { readonly namespace: RouteNamespace; readonly key: string } | undefined
  readonly override: (routeKey: string) => string | undefined
  /** Original name of the printing's front face, for the canonical slug. */
  readonly nameOf: (printingId: string) => string | undefined
}

export type RouteResolution =
  | { readonly kind: "found"; readonly printingId: string; readonly canonicalPath: string }
  | { readonly kind: "redirect"; readonly to: string }
  | { readonly kind: "missing" }

export function cardPath(cardNo: string, name?: string): string {
  const base = `/cards/${encodeURIComponent(cardNo)}`
  return name === undefined || name === "" ? base : `${base}/${encodeURIComponent(name)}`
}

function provisionalPath(intId: number): string {
  return `/cards/_provisional/${String(intId)}`
}

function target(namespace: RouteNamespace, key: string): string {
  return namespace === "provisional" ? provisionalPath(Number(key)) : cardPath(key)
}

/** Resolves an official card number (as typed in the URL, already decoded) with its optional slug. */
export function resolveCardRoute(
  cardNo: string,
  slug: string | undefined,
  lookups: RouteLookups,
): RouteResolution {
  const alias = lookups.alias("official", cardNo)
  if (alias) return { kind: "redirect", to: target(alias.namespace, alias.key) }
  const printingId = lookups.override(cardNo) ?? lookups.printingByCardNo(cardNo)
  if (printingId === undefined) return { kind: "missing" }
  const canonicalPath = cardPath(cardNo, lookups.nameOf(printingId))
  if (slug !== undefined && cardPath(cardNo, slug) === canonicalPath)
    return { kind: "found", printingId, canonicalPath }
  return slug === undefined && canonicalPath === cardPath(cardNo)
    ? { kind: "found", printingId, canonicalPath }
    : { kind: "redirect", to: canonicalPath }
}

export function resolveProvisionalRoute(intId: string, lookups: RouteLookups): RouteResolution {
  if (!/^\d+$/u.test(intId)) return { kind: "missing" }
  const alias = lookups.alias("provisional", intId)
  if (alias) return { kind: "redirect", to: target(alias.namespace, alias.key) }
  const printingId = lookups.printingByIntId(Number(intId))
  return printingId === undefined
    ? { kind: "missing" }
    : { kind: "found", printingId, canonicalPath: provisionalPath(Number(intId)) }
}
