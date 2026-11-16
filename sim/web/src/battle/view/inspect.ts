// What a seat may read about one card it can identify: where it lies and its current state. Like
// the table model, nothing here works out a rule; every value is the authority's.
import type { BattleObservation, CardRef, ObjectHandle, Seat, Zones } from "../contract"
import { type TableCard, tableCard } from "./table"

export type ZoneName = keyof Zones

export interface Inspection {
  readonly card: TableCard
  /** Whose zone the card lies in, which is not always who owns it. */
  readonly seat: Seat
  readonly zone: ZoneName
  /** The follower under an evolve card; null when the card shows its own face. */
  readonly under: CardRef | null
  /** Effective traits, as the rules terms of the card text. */
  readonly traits: readonly string[]
}

const sameFace = (a: CardRef, b: CardRef) => a.printing_id === b.printing_id && a.face === b.face

/** Null for a handle this seat cannot identify, or one no zone lists any more. */
export function inspect(observation: BattleObservation, handle: ObjectHandle): Inspection | null {
  const view = observation.objects[handle]
  const card = tableCard(observation, handle)
  if (view === undefined || card === null) return null
  for (const player of observation.players) {
    const zones: Record<string, Zones[ZoneName]> = { ...player.zones }
    for (const [zone, members] of Object.entries(zones)) {
      if (!members.cards.some((member) => member.object === handle)) continue
      return {
        card,
        seat: player.seat,
        zone: zone as ZoneName,
        under: sameFace(card.card, card.face) ? null : card.card,
        traits: view.characteristics.traits,
      }
    }
  }
  return null
}
