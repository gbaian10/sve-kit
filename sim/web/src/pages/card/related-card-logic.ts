import type { Row } from "../../data"
import type { CardKind } from "./related-cards"

const sameName = (a: string | undefined, b: string | undefined): boolean =>
  a !== undefined && b !== undefined && a.trim() === b.trim()

/**
 * An official relation is an evolve pair when exactly one side is the evolve card and both carry
 * the same name, or when exactly one side is an advance card and the other is a plain follower
 * (advance cards are renamed, so the name cannot decide). Tokens, evolve-to-evolve rows and
 * same-name cards of other types are references.
 */
export function isEvolvePair(a: CardKind, b: CardKind): boolean {
  if (a.evolve !== b.evolve) return sameName(a.name, b.name)
  if (a.advance !== b.advance) {
    const base = a.advance ? b : a
    return base.follower && !base.token && !base.evolve && !base.advance
  }
  return false
}

export const plainFollower = (kind: CardKind): boolean =>
  kind.follower && !kind.token && !kind.evolve && !kind.advance

const targetsOf = (rows: readonly Row[]): string[] => [
  ...new Set(
    rows.flatMap((row) => (typeof row["to_card_id"] === "string" ? [row["to_card_id"]] : [])),
  ),
]

/**
 * The base of a renamed evolve card: with no same-named partner at all, the one plain follower
 * among its official relations. None or several plain followers decide nothing.
 */
export function fallbackBase(
  self: CardKind,
  related: readonly Row[],
  kind: (cardId: string) => CardKind | undefined,
): string | undefined {
  if (!self.evolve) return undefined
  const kinds = targetsOf(related).map((id) => ({ id, kind: kind(id) }))
  if (
    kinds.some((entry) => entry.kind && !entry.kind.evolve && sameName(self.name, entry.kind.name))
  )
    return undefined
  const plain = kinds.filter((entry) => entry.kind && plainFollower(entry.kind))
  return plain.length === 1 ? plain[0]?.id : undefined
}

export type EvolveDirection = "from" | "into"

/** An evolve or advance card lists what it evolves from; any other card lists what it evolves into. */
export const evolveDirection = (kind: CardKind): EvolveDirection =>
  kind.evolve || kind.advance ? "from" : "into"
