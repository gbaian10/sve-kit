import type { Region } from "./search"

export interface RoutePrinting {
  readonly id: string
  readonly cardId: string
  readonly cardOwner: string
  readonly printingOwner: string
  readonly region: Region
  readonly cardNo: string
  readonly cardNoState: string
  readonly intId?: number
}
export interface RouteAlias {
  readonly namespace: string
  readonly oldKey: string
  readonly targetNamespace: string
  readonly targetKey: string
}
export class CardRouteError extends Error {
  readonly kind:
    "not-found" | "target-not-found" | "needs-update" | "ambiguous" | "invalid" | "reference"
  constructor(kind: CardRouteError["kind"], message: string) {
    super(message)
    this.kind = kind
  }
}
export const routeFold = (value: string): string => value.normalize("NFKC").toLowerCase()
function validRouteKey(key: string): boolean {
  return key !== "" && !key.includes(String.fromCharCode(0)) && !/[\u002F\uD800-\uDFFF]/u.test(key)
}
export function printingPath(printing: {
  readonly cardNo: string
  readonly cardNoState?: string
  readonly intId?: number
}): string {
  return printing.cardNoState === "provisional" && printing.intId !== undefined
    ? `/cards/_provisional/${String(printing.intId)}`
    : `/cards/${encodeURIComponent(printing.cardNo).replace(/[!'()*]/gu, (value) => `%${value.charCodeAt(0).toString(16).toUpperCase()}`)}`
}

/** Exact active keys precede permanent aliases, then a unique folded key. Slugs never identify cards. */
export function resolveRoute(
  input: { readonly namespace: "official" | "provisional"; readonly key: string },
  printings: readonly RoutePrinting[],
  aliases: readonly RouteAlias[],
  overrides: ReadonlyMap<string, string>,
): RoutePrinting {
  if (!validRouteKey(input.key)) throw new CardRouteError("invalid", "invalid route key")
  const active = (namespace: string, key: string): RoutePrinting | undefined => {
    const matches = printings.filter((p) =>
      namespace === "provisional"
        ? p.cardNoState === "provisional" && String(p.intId) === key
        : p.cardNoState === "official" && p.cardNo === key,
    )
    if (namespace === "official" && overrides.has(key)) {
      const selected = matches.find((p) => p.id === overrides.get(key))
      if (!selected) {
        const kind = printings.some((p) => p.id === overrides.get(key))
          ? "reference"
          : "target-not-found"
        throw new CardRouteError(kind, "route override has no matching printing")
      }
      return selected
    }
    if (matches.length > 1) throw new CardRouteError("ambiguous", "card route is ambiguous")
    return matches[0]
  }
  let namespace: string = input.namespace
  let key = input.key
  const seen = new Set<string>()
  for (;;) {
    const identity = `${namespace}/${key}`
    if (seen.has(identity)) throw new CardRouteError("reference", "cyclic card route alias")
    seen.add(identity)
    const exact = active(namespace, key)
    if (exact) return exact
    const targets = aliases.filter((alias) => alias.namespace === namespace && alias.oldKey === key)
    if (targets.length > 1) throw new CardRouteError("reference", "conflicting card route alias")
    const alias = targets[0]
    if (!alias) break
    namespace = alias.targetNamespace
    key = alias.targetKey
  }
  if (seen.size > 1)
    throw new CardRouteError("target-not-found", "card route alias target is missing")
  if (namespace === "provisional") {
    if (!/^[1-9][0-9]*$/u.test(key) || Number(key) > 0xffffffff)
      throw new CardRouteError("invalid", "invalid printing integer")
    // A corrected provisional identity remains useful even when its old alias was not needed locally.
    const corrected = printings.filter((p) => String(p.intId) === key)
    if (corrected.length === 1 && corrected[0]) return corrected[0]
    if (corrected.length > 1) throw new CardRouteError("reference", "duplicate printing integer")
    throw new CardRouteError("needs-update", "unknown printing integer; update card data")
  }
  const folded = [
    ...new Set(
      printings
        .filter((p) => p.cardNoState === "official" && routeFold(p.cardNo) === routeFold(key))
        .map((p) => p.cardNo),
    ),
  ]
  if (folded.length > 1) throw new CardRouteError("ambiguous", "folded card route is ambiguous")
  if (folded[0]) {
    const match = active("official", folded[0])
    if (match) return match
  }
  throw new CardRouteError("not-found", "card number not found")
}
