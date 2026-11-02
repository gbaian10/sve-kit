import { NEUTRAL_CLASS } from "../query/model"
import { EVOLVE_MAX, MAIN_MAX, MAIN_MIN, placementOf, SAME_NAME_LIMIT, totalCount } from "./model"
import type { Deck, DeckCardInfo, DeckRestriction } from "./types"

type ProblemSeverity = "error" | "warning" | "info"

type DeckProblemKind =
  | "leaderMissing"
  | "leaderNotLeader"
  | "leaderClassMismatch"
  | "leaderTitleMismatch"
  | "mainTooFew"
  | "mainTooMany"
  | "evolveTooMany"
  | "mainWrongCard"
  | "evolveWrongCard"
  | "tokenInDeck"
  | "sameNameOver"
  | "classMismatch"
  | "classMismatchNeutral"
  | "titleMismatch"
  | "restricted"
  | "banned"
  | "unknownCard"
  | "titleRulesUnchecked"

export interface DeckProblem {
  readonly kind: DeckProblemKind
  readonly severity: ProblemSeverity
  /** Comprehensive Rules clause (ver. 1.28.0), or `restriction` for the official Standard list. */
  readonly clause: string
  /** Cards the message is about; the deck list scrolls to the first. */
  readonly cardIds: readonly string[]
  readonly values: Readonly<Record<string, string | number>>
}

export interface RulesContext {
  readonly infoOf: (cardId: string) => DeckCardInfo | undefined
  readonly restrictions: ReadonlyMap<string, readonly DeckRestriction[]>
}

// CR 14.4: Vanguard title decks have construction rules of their own that are not checked yet.
const TITLES_WITH_OWN_RULES: ReadonlySet<string> = new Set(["cardfight_vanguard"])

const isNeutral = (code: string | null): boolean => code === null || code === NEUTRAL_CLASS

const problem = (
  kind: DeckProblemKind,
  severity: ProblemSeverity,
  clause: string,
  cardIds: readonly string[] = [],
  values: Readonly<Record<string, string | number>> = {},
): DeckProblem => ({ kind, severity, clause, cardIds, values })

/** Every construction problem of a deck, errors first; the side list is never checked. */
export function checkDeck(deck: Deck, { infoOf, restrictions }: RulesContext): DeckProblem[] {
  const problems: DeckProblem[] = []
  const nameOf = (cardId: string) => infoOf(cardId)?.name ?? cardId
  const mainCount = totalCount(deck.main)
  const evolveCount = totalCount(deck.evolve)

  // 6.1.1.1 one leader card
  const leaderId = deck.leader?.cardId
  const leaderInfo = leaderId === undefined ? undefined : infoOf(leaderId)
  if (leaderId === undefined) problems.push(problem("leaderMissing", "warning", "6.1.1.1"))
  else if (!leaderInfo)
    problems.push(problem("unknownCard", "warning", "6.1.1.1", [leaderId], { cardId: leaderId }))
  else if (leaderInfo.typeCode !== "leader")
    problems.push(
      problem("leaderNotLeader", "error", "6.1.1.1", [leaderId], { name: leaderInfo.name }),
    )

  // 6.1.1.2 / 6.1.1.3 sizes
  if (mainCount < MAIN_MIN)
    problems.push(
      problem("mainTooFew", "error", "6.1.1.2", [], {
        count: mainCount,
        min: MAIN_MIN,
        max: MAIN_MAX,
      }),
    )
  if (mainCount > MAIN_MAX)
    problems.push(
      problem("mainTooMany", "error", "6.1.1.2", [], { count: mainCount, max: MAIN_MAX }),
    )
  if (evolveCount > EVOLVE_MAX)
    problems.push(
      problem("evolveTooMany", "error", "6.1.1.3", [], { count: evolveCount, max: EVOLVE_MAX }),
    )

  const sections = [
    ["main", deck.main],
    ["evolve", deck.evolve],
  ] as const
  // 6.1.1.2 / 6.1.1.3 cards in the wrong deck; tokens never go in
  for (const [section, entries] of sections)
    for (const entry of entries) {
      const info = infoOf(entry.cardId)
      if (!info) {
        problems.push(
          problem("unknownCard", "warning", "6.1.1", [entry.cardId], { cardId: entry.cardId }),
        )
        continue
      }
      if (info.specialKinds.includes("token"))
        problems.push(
          problem("tokenInDeck", "error", "6.1.1.2", [entry.cardId], { name: info.name }),
        )
      else if (placementOf(info) !== section)
        problems.push(
          problem(
            section === "main" ? "mainWrongCard" : "evolveWrongCard",
            "error",
            section === "main" ? "6.1.1.2" : "6.1.1.3",
            [entry.cardId],
            { name: info.name },
          ),
        )
    }

  // 6.1.1.4 three per card name in each deck
  for (const [section, entries] of sections) {
    const byName = new Map<string, { count: number; cardIds: string[] }>()
    for (const entry of entries) {
      const name = nameOf(entry.cardId)
      const group = byName.get(name) ?? { count: 0, cardIds: [] }
      group.count += entry.count
      group.cardIds.push(entry.cardId)
      byName.set(name, group)
    }
    for (const [name, group] of byName)
      if (group.count > SAME_NAME_LIMIT)
        problems.push(
          problem("sameNameOver", "error", "6.1.1.4", group.cardIds, {
            name,
            count: group.count,
            limit: SAME_NAME_LIMIT,
            section,
          }),
        )
  }

  // 6.1.1.5 the basis: a class (6.1.1.5.1) or a title (6.1.1.5.2)
  const everyEntry = [...deck.main, ...deck.evolve]
  if (deck.basis.kind === "class") {
    const deckClass = deck.basis.classCode
    // A neutral basis is the builder's own option (no CR clause): neutral cards with any leader.
    const neutralDeck = deckClass === NEUTRAL_CLASS
    if (
      !neutralDeck &&
      leaderId !== undefined &&
      leaderInfo?.typeCode === "leader" &&
      leaderInfo.classCode !== deckClass
    )
      problems.push(
        problem("leaderClassMismatch", "error", "6.1.1.5.1", [leaderId], {
          leaderClass: leaderInfo.classCode ?? NEUTRAL_CLASS,
          deckClass,
        }),
      )
    for (const entry of everyEntry) {
      const info = infoOf(entry.cardId)
      if (!info || isNeutral(info.classCode) || info.classCode === deckClass) continue
      problems.push(
        problem(
          neutralDeck ? "classMismatchNeutral" : "classMismatch",
          "error",
          "6.1.1.5.1",
          [entry.cardId],
          { name: info.name, cardClass: info.classCode ?? NEUTRAL_CLASS, deckClass },
        ),
      )
    }
  } else {
    const title = deck.basis.titleCode
    if (leaderId !== undefined && leaderInfo && !leaderInfo.titles.includes(title))
      problems.push(problem("leaderTitleMismatch", "error", "6.1.1.5.2", [leaderId], { title }))
    for (const entry of everyEntry) {
      const info = infoOf(entry.cardId)
      if (!info || info.titles.includes(title)) continue
      problems.push(
        problem("titleMismatch", "error", "6.1.1.5.2", [entry.cardId], { name: info.name, title }),
      )
    }
    if (TITLES_WITH_OWN_RULES.has(title))
      problems.push(problem("titleRulesUnchecked", "info", "14.4", [], { title }))
  }

  // Official Standard restrictions bind every card of one restriction across both decks.
  const byRestriction = new Map<string, { count: number; cardIds: string[]; max: number }>()
  for (const entry of everyEntry)
    for (const restriction of restrictions.get(entry.cardId) ?? []) {
      if (restriction.maxCopies === null) continue
      const group = byRestriction.get(restriction.id) ?? {
        count: 0,
        cardIds: [],
        max: restriction.maxCopies,
      }
      group.count += entry.count
      if (!group.cardIds.includes(entry.cardId)) group.cardIds.push(entry.cardId)
      byRestriction.set(restriction.id, group)
    }
  for (const group of byRestriction.values()) {
    if (group.count <= group.max) continue
    const name = nameOf(group.cardIds[0] ?? "")
    problems.push(
      problem(group.max === 0 ? "banned" : "restricted", "error", "restriction", group.cardIds, {
        name,
        count: group.count,
        max: group.max,
      }),
    )
  }

  const rank: Record<ProblemSeverity, number> = { error: 0, warning: 1, info: 2 }
  return problems.sort((a, b) => rank[a.severity] - rank[b.severity])
}
