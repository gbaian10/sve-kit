// The questions that arrange things or ask for amounts, as the screen works with them: an order,
// a split into piles, amounts dealt out among recipients, one number. Like the other readers they
// repeat what the question says and build its answer. Their hints only hold the answer so far
// against the numbers the question states; whether it is legal stays the authority's to say.
import type {
  Bounds,
  Candidate,
  Command,
  Destination,
  EmptyDistribution,
  OptionId,
  OrderDirection,
  OrderItem,
  PartitionBucket,
  QuantityUnit,
  Question,
  QuestionId,
  Seat,
  SourceRef,
} from "../contract"

const outside = (have: number, bounds: Bounds): boolean =>
  have < bounds.min || (bounds.max !== null && have > bounds.max)

export interface Ordering {
  readonly question: QuestionId
  /** The cards or abilities to put in order. */
  readonly items: readonly OrderItem[]
  /** How the order is read: from the top down, or as the order things are applied in. */
  readonly direction: OrderDirection
  /** Where ordered cards go, when the question says. */
  readonly destination: Destination | null
}

/** The ordering put to `viewer`; null for anything else. */
export function ordering(question: Question | null, viewer: Seat): Ordering | null {
  if (question?.actor !== viewer || question.shape.kind !== "order") return null
  const { items, direction, destination } = question.shape
  return { question: question.id, items, direction, destination }
}

/** Whether an order names every item exactly once, as the answer has to. */
export function wholeOrder(asked: Ordering, order: readonly OptionId[]): boolean {
  return (
    order.length === asked.items.length &&
    asked.items.every((item) => order.filter((id) => id === item.id).length === 1)
  )
}

export function orderAnswer(asked: Ordering, order: readonly OptionId[]): Command {
  return {
    kind: "answer",
    question_id: asked.question,
    answer: { kind: "order", order: [...order] },
  }
}

export interface Partition {
  readonly question: QuestionId
  /** The complete set to split. */
  readonly items: readonly Candidate[]
  /** Every pile, the one for what is left over included. */
  readonly buckets: readonly PartitionBucket[]
}

/** The split put to `viewer`; null for anything else. */
export function partition(question: Question | null, viewer: Seat): Partition | null {
  if (question?.actor !== viewer || question.shape.kind !== "partition") return null
  return { question: question.id, items: question.shape.items, buckets: question.shape.buckets }
}

/** The items of each pile so far, in the order they were put there. */
export type Piles = ReadonlyMap<number, readonly OptionId[]>

export const NO_PILES: Piles = new Map()

/**
 * The piles with one item put on a pile, at its end. An item lies on one pile only: put on
 * another it leaves the first, and put on the pile it is already on it goes to that pile's end.
 * A pile of null takes the item off every pile.
 */
export function put(asked: Partition, piles: Piles, item: OptionId, bucket: number | null): Piles {
  if (!asked.items.some((each) => each.id === item)) return piles
  if (bucket !== null && !asked.buckets.some((each) => each.bucket === bucket)) return piles
  const next = new Map<number, readonly OptionId[]>()
  for (const { bucket: each } of asked.buckets) {
    const kept = (piles.get(each) ?? []).filter((held) => held !== item)
    const now = each === bucket ? [...kept, item] : kept
    if (now.length > 0) next.set(each, now)
  }
  return next
}

/** Something the question's own numbers say about the piles as they stand. */
export type PileProblem =
  /** An item lies on no pile yet. */
  | { readonly kind: "unplaced"; readonly option: OptionId }
  /** A pile holds fewer or more items than it takes. */
  | {
      readonly kind: "pile"
      readonly bucket: number
      readonly have: number
      readonly bounds: Bounds
    }

export function pileProblems(asked: Partition, piles: Piles): PileProblem[] {
  const placed = new Set([...piles.values()].flat())
  return [
    ...asked.items.flatMap((item): PileProblem[] =>
      placed.has(item.id) ? [] : [{ kind: "unplaced", option: item.id }],
    ),
    ...asked.buckets.flatMap((bucket): PileProblem[] => {
      const have = piles.get(bucket.bucket)?.length ?? 0
      return outside(have, bucket.bounds)
        ? [{ kind: "pile", bucket: bucket.bucket, have, bounds: bucket.bounds }]
        : []
    }),
  ]
}

/** The command that answers with the piles: every pile the question lists, the empty ones too. */
export function partitionAnswer(asked: Partition, piles: Piles): Command {
  return {
    kind: "answer",
    question_id: asked.question,
    answer: {
      kind: "partition",
      buckets: asked.buckets.map(({ bucket }) => ({
        bucket,
        items: [...(piles.get(bucket) ?? [])],
      })),
    },
  }
}

export interface Distribution {
  readonly question: QuestionId
  /** Who the amounts are dealt out among. */
  readonly recipients: readonly Candidate[]
  /** What one recipient may get. */
  readonly share: Bounds
  /** What has to be dealt out in all. */
  readonly total: number
  /** What it means when there is nobody to deal to. */
  readonly empty: EmptyDistribution
}

/** The dealing-out put to `viewer`; null for anything else. */
export function distribution(question: Question | null, viewer: Seat): Distribution | null {
  if (question?.actor !== viewer || question.shape.kind !== "distribute") return null
  const { recipients, share, total, empty } = question.shape
  return { question: question.id, recipients, share, total, empty }
}

/** The amount given to each recipient so far; a recipient without one has none. */
export type Shares = ReadonlyMap<OptionId, number>

/** Something the question's own numbers say about the amounts as they stand. */
export type ShareProblem =
  /** There is nobody to deal to, and the question takes no answer then. */
  | { readonly kind: "nobody" }
  /** The amounts do not add up to what has to be dealt out. */
  | { readonly kind: "total"; readonly have: number; readonly want: number }
  /** One recipient gets less or more than a recipient may, or an amount that is no count. */
  | {
      readonly kind: "share"
      readonly option: OptionId
      readonly have: number
      readonly bounds: Bounds
    }

export function shareProblems(asked: Distribution, shares: Shares): ShareProblem[] {
  // With nobody to deal to, the total does not matter: the question says whether the empty
  // answer is taken, and takes none at all when it is not.
  if (asked.recipients.length === 0) return asked.empty === "reject" ? [{ kind: "nobody" }] : []
  const amounts = asked.recipients.map((each) => ({ id: each.id, have: shares.get(each.id) ?? 0 }))
  const have = amounts.reduce((sum, each) => sum + each.have, 0)
  return [
    ...(have === asked.total ? [] : [{ kind: "total" as const, have, want: asked.total }]),
    ...amounts.flatMap((each): ShareProblem[] =>
      !Number.isSafeInteger(each.have) || outside(each.have, asked.share)
        ? [{ kind: "share", option: each.id, have: each.have, bounds: asked.share }]
        : [],
    ),
  ]
}

/** The command that answers with the amounts: every recipient once, none of them left out. */
export function distributeAnswer(asked: Distribution, shares: Shares): Command {
  return {
    kind: "answer",
    question_id: asked.question,
    answer: {
      kind: "distribute",
      shares: asked.recipients.map(({ id }) => ({ option: id, amount: shares.get(id) ?? 0 })),
    },
  }
}

export interface Integer {
  readonly question: QuestionId
  readonly bounds: Bounds
  /** What the number counts, when the question says. */
  readonly unit: QuantityUnit | null
  /** The text that asks for it, when the question says. */
  readonly source: SourceRef | null
}

/** The number asked of `viewer`; null for anything else. */
export function integer(question: Question | null, viewer: Seat): Integer | null {
  if (question?.actor !== viewer || question.shape.kind !== "integer") return null
  const { bounds, unit, source } = question.shape
  return { question: question.id, bounds, unit, source }
}

/**
 * Whether a number is one the question takes: within its range, and a whole number the contract
 * can carry. A range without an upper end still stops at the largest such number.
 */
export function inRange(asked: Integer, value: number): boolean {
  return Number.isSafeInteger(value) && !outside(value, asked.bounds)
}

export function integerAnswer(asked: Integer, value: number): Command {
  return { kind: "answer", question_id: asked.question, answer: { kind: "integer", value } }
}
