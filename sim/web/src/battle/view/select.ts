// A select question as the screen works with it: what may be picked, what has been picked so far,
// what the question's own numbers say is missing or too much, and the answer the picks make.
// Nothing here judges a pick by the rules. The hints only hold the picks against the counts and
// the must-differ properties the question states; whether the answer is legal stays the
// authority's to say.
import type {
  Bounds,
  Command,
  Distinct,
  ObjectHandle,
  OptionId,
  Question,
  QuestionId,
  Seat,
  Selection,
  Subject,
} from "../contract"

export interface Select {
  readonly question: QuestionId
  readonly selection: Selection
}

/** The selection put to `viewer`; null for anything else. */
export function selection(question: Question | null, viewer: Seat): Select | null {
  if (question?.actor !== viewer || question.shape.kind !== "select") return null
  return { question: question.id, selection: question.shape.selection }
}

/** What a subject is on this seat's table: a card it can point at, a leader, or neither. */
export type OnTable =
  | { readonly kind: "card"; readonly handle: ObjectHandle }
  | { readonly kind: "leader"; readonly seat: Seat }

/**
 * Where a candidate or a shown subject is to be found on the table. Null for a subject the seat
 * may not know and for a card it knows but cannot point at, such as one inside a deck: those are
 * shown beside the table instead.
 */
export function onTable(subject: Subject | null): OnTable | null {
  if (subject === null) return null
  if (subject.kind !== "card") return { kind: "leader", seat: subject.player }
  return subject.object === null ? null : { kind: "card", handle: subject.object }
}

export interface Picks {
  /** Picked candidates in the order they were picked, which is the order the answer lists them in. */
  readonly selected: readonly OptionId[]
  /** The group each pick is given, where the question wants the groups named. */
  readonly groups: ReadonlyMap<OptionId, number>
  /** The role each pick is given; a pick without one falls to the role that takes the remainder. */
  readonly roles: ReadonlyMap<OptionId, number>
}

export const NO_PICKS: Picks = { selected: [], groups: new Map(), roles: new Map() }

const dropped = <V>(map: ReadonlyMap<OptionId, V>, option: OptionId): ReadonlyMap<OptionId, V> => {
  const next = new Map(map)
  next.delete(option)
  return next
}

/**
 * The picks with one candidate picked, or put back if it was picked already. A question that
 * takes a single pick swaps it for the new one; any other question at its maximum takes no more.
 * Where groups are to be named, a new pick goes to the first group that accepts it; `give` moves it.
 */
export function toggle(asked: Select, picks: Picks, option: OptionId): Picks {
  const { bounds, candidates, group_assignment, groups } = asked.selection
  if (!candidates.some((candidate) => candidate.id === option)) return picks
  if (picks.selected.includes(option))
    return {
      selected: picks.selected.filter((picked) => picked !== option),
      groups: dropped(picks.groups, option),
      roles: dropped(picks.roles, option),
    }
  const full = bounds.max !== null && picks.selected.length >= bounds.max
  if (full && bounds.max !== 1) return picks
  const kept = full ? NO_PICKS : picks
  const group =
    group_assignment === "explicit"
      ? groups.find((each) => each.eligible.includes(option))?.group
      : undefined
  return {
    selected: [...kept.selected, option],
    groups: group === undefined ? kept.groups : new Map(kept.groups).set(option, group),
    roles: kept.roles,
  }
}

/** The picks with one of them given a group or a role; a role of null leaves it to the remainder. */
export function give(
  picks: Picks,
  option: OptionId,
  to: { readonly group: number } | { readonly role: number | null },
): Picks {
  if (!picks.selected.includes(option)) return picks
  if ("group" in to) return { ...picks, groups: new Map(picks.groups).set(option, to.group) }
  return {
    ...picks,
    roles:
      to.role === null ? dropped(picks.roles, option) : new Map(picks.roles).set(option, to.role),
  }
}

/** Something the question's own numbers say about the picks as they stand. */
export type Problem =
  /** Fewer or more picks than the question takes. */
  | { readonly kind: "count"; readonly have: number; readonly bounds: Bounds }
  /** A named group holds fewer or more picks than it takes. */
  | {
      readonly kind: "group"
      readonly group: number
      readonly have: number
      readonly bounds: Bounds
    }
  /** A pick is in no group, or in one that does not accept it. */
  | { readonly kind: "ungrouped"; readonly option: OptionId }
  /** A role holds fewer or more picks than its size. */
  | { readonly kind: "role"; readonly role: number; readonly have: number; readonly bounds: Bounds }
  /** A pick has no role and no role takes the remainder. */
  | { readonly kind: "unassigned"; readonly option: OptionId }
  /** Picks share a property that has to differ between them. */
  | { readonly kind: "same"; readonly property: Distinct; readonly options: readonly OptionId[] }

const outside = (have: number, bounds: Bounds): boolean =>
  have < bounds.min || (bounds.max !== null && have > bounds.max)

/** The role that takes whatever no other role was given, when the question has one. */
const remainderOf = (asked: Select): number | undefined =>
  asked.selection.roles.find((role) => role.size.kind === "remainder")?.role

/**
 * What is missing, too much or doubled in the picks, by the question's own statement. An empty
 * list does not make the answer legal; it only means these counts have nothing against it.
 */
export function problems(asked: Select, picks: Picks): Problem[] {
  const { bounds, candidates, distinct, group_assignment, groups, roles } = asked.selection
  const found: Problem[] = []
  const have = picks.selected.length
  if (outside(have, bounds)) found.push({ kind: "count", have, bounds })
  if (group_assignment === "explicit") {
    for (const group of groups) {
      const held = picks.selected.filter((option) => picks.groups.get(option) === group.group)
      if (outside(held.length, group.bounds))
        found.push({ kind: "group", group: group.group, have: held.length, bounds: group.bounds })
    }
    for (const option of picks.selected) {
      const group = groups.find((each) => each.group === picks.groups.get(option))
      if (group?.eligible.includes(option) !== true) found.push({ kind: "ungrouped", option })
    }
  }
  const remainder = remainderOf(asked)
  for (const role of roles) {
    if (role.size.kind === "remainder") continue
    const held = picks.selected.filter((option) => picks.roles.get(option) === role.role).length
    // A role that exists only for a selection that is not empty takes exactly its count then.
    const want: Bounds =
      role.size.kind === "fixed"
        ? role.size.bounds
        : have === 0
          ? { min: 0, max: 0 }
          : { min: role.size.count, max: role.size.count }
    if (outside(held, want)) found.push({ kind: "role", role: role.role, have: held, bounds: want })
  }
  if (roles.length > 0 && remainder === undefined)
    for (const option of picks.selected)
      if (!picks.roles.has(option)) found.push({ kind: "unassigned", option })
  const picked = candidates.filter((candidate) => picks.selected.includes(candidate.id))
  const shares = (property: Distinct, a: (typeof picked)[number], b: (typeof picked)[number]) =>
    property === "original_cost"
      ? a.original_cost !== null && a.original_cost === b.original_cost
      : property === "rules_name"
        ? (a.rules_names ?? []).some((name) => (b.rules_names ?? []).includes(name))
        : (a.classes ?? []).some((name) => (b.classes ?? []).includes(name))
  for (const property of distinct) {
    const doubled = picked.filter((one) =>
      picked.some((other) => other.id !== one.id && shares(property, one, other)),
    )
    if (doubled.length > 0)
      found.push({ kind: "same", property, options: doubled.map((candidate) => candidate.id) })
  }
  return found
}

/**
 * The sum of the picks' original costs beside the limits the question puts on it, for the screen
 * to show. The sum is null when a pick's original cost is not given.
 */
export function costTotal(asked: Select, picks: Picks) {
  const { candidates, original_cost_total: limits } = asked.selection
  const costs = candidates
    .filter((candidate) => picks.selected.includes(candidate.id))
    .map((candidate) => candidate.original_cost)
  return {
    sum: costs.includes(null) ? null : costs.reduce<number>((sum, cost) => sum + (cost ?? 0), 0),
    min: limits.min,
    max: limits.max,
  }
}

/** The command that answers with the picks: every group and every role the question lists. */
export function selectAnswer(asked: Select, picks: Picks): Command {
  const { group_assignment, groups, roles } = asked.selection
  const remainder = remainderOf(asked)
  return {
    kind: "answer",
    question_id: asked.question,
    answer: {
      kind: "select",
      selected: [...picks.selected],
      groups:
        group_assignment === "explicit"
          ? groups.map(({ group }) => ({
              group,
              selected: picks.selected.filter((option) => picks.groups.get(option) === group),
            }))
          : null,
      roles: roles.map(({ role }) => ({
        role,
        selected: picks.selected.filter(
          (option) => (picks.roles.get(option) ?? remainder) === role,
        ),
      })),
    },
  }
}
