// Looking at cards: which one the info panel shows, and how the parts of the table ask for it.
import { createContext, type KeyboardEvent, type PointerEvent, use } from "react"

import type { CardRef, ObjectHandle, Seat } from "../../battle/contract"

/** Something on the table a player can look at: a card they can identify, or a leader. */
export type Picked =
  | { readonly kind: "object"; readonly handle: ObjectHandle }
  | { readonly kind: "leader"; readonly seat: Seat }

/** The piles whose cards can be browsed; a deck is never public. */
export type BrowsablePile = "cemetery" | "banish" | "evolveDeck"

/** A card shown large: its face in effect and, for an evolved follower, the card underneath. */
export interface Zoomed {
  readonly face: CardRef
  readonly under: CardRef | null
}

/** What the picked card may attack; these light up for as long as it is picked. */
export interface Targets {
  readonly leader: Seat | null
  readonly objects: readonly ObjectHandle[]
}

export const NO_TARGETS: Targets = { leader: null, objects: [] }

export interface Inspector {
  readonly picked: Picked | null
  readonly targets: Targets
  readonly select: (picked: Picked) => void
  /** What the pointer rests on; shown instead of the selection until it leaves. */
  readonly preview: (picked: Picked | null) => void
  readonly zoom: (zoomed: Zoomed) => void
  readonly browse: (seat: Seat, pile: BrowsablePile) => void
}

/** The key that pins and unpins the info column, as printed on its button. */
export const PIN_KEY = "F1"

/** Null where nothing can be picked: the other seat's screen in the dual view is only shown. */
export const InspectorContext = createContext<Inspector | null>(null)

export const samePick = (a: Picked | null, b: Picked | null): boolean =>
  a !== null &&
  b !== null &&
  (a.kind === "object"
    ? b.kind === "object" && a.handle === b.handle
    : b.kind === "leader" && a.seat === b.seat)

/**
 * What makes an element of the table pick its card: a click or Enter selects it, the mouse resting
 * on it previews it. Touch never previews, or a tap would preview and select at once.
 */
export function usePick(target: Picked | null) {
  const inspector = use(InspectorContext)
  if (inspector === null || target === null) return { selected: false, targeted: false, props: {} }
  const mouse =
    (run: () => void) =>
    (event: PointerEvent): void => {
      if (event.pointerType === "mouse") run()
    }
  return {
    selected: samePick(inspector.picked, target),
    targeted:
      target.kind === "object"
        ? inspector.targets.objects.includes(target.handle)
        : inspector.targets.leader === target.seat,
    props: {
      role: "button",
      tabIndex: 0,
      "aria-pressed": samePick(inspector.picked, target),
      onClick: () => {
        inspector.select(target)
      },
      onKeyDown: (event: KeyboardEvent) => {
        if (event.key !== "Enter" && event.key !== " ") return
        event.preventDefault()
        inspector.select(target)
      },
      onPointerEnter: mouse(() => {
        inspector.preview(target)
      }),
      onPointerLeave: mouse(() => {
        inspector.preview(null)
      }),
    },
  }
}
