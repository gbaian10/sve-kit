// The snapshot tags cards with mechanism codes but not with the class or title a mechanism belongs
// to, so the filter's rows are listed here. Each class mechanism is printed only on its class's
// cards (checked against the JP card text in card-data); each crossover mechanism comes from one
// collaboration: Serve from CP01 Umamusume, Lesson from CP02 Cinderella Girls, Starting Amulet and
// both Drives from CP03 Vanguard. Title codes are the snapshot's title vocabulary.
export const CLASS_MECHANISM_GROUPS = [
  { classCode: "elf", mechanisms: ["combo"] },
  { classCode: "witch", mechanisms: ["spellchain", "stack", "earth_rite"] },
  { classCode: "dragon", mechanisms: ["overflow"] },
  { classCode: "nightmare", mechanisms: ["necrocharge", "sanguine"] },
] as const
export const COLLAB_MECHANISM_GROUPS = [
  { titleCode: "umamusume_pretty_derby", mechanisms: ["serve"] },
  { titleCode: "idolmaster_cinderella_girls", mechanisms: ["lesson"] },
  {
    titleCode: "cardfight_vanguard",
    mechanisms: ["starting_amulet", "single_drive", "twin_drive"],
  },
] as const
export type ClassMechanism = (typeof CLASS_MECHANISM_GROUPS)[number]["mechanisms"][number]
export type CollabMechanism = (typeof COLLAB_MECHANISM_GROUPS)[number]["mechanisms"][number]
export const CLASS_MECHANISMS: readonly ClassMechanism[] = CLASS_MECHANISM_GROUPS.flatMap(
  (group) => group.mechanisms,
)
export const COLLAB_MECHANISMS: readonly CollabMechanism[] = COLLAB_MECHANISM_GROUPS.flatMap(
  (group) => group.mechanisms,
)
export type PrintedMechanism = ClassMechanism | CollabMechanism
/** Mechanisms cards also print by a short form (SC, NC); the filter explains it on hover. */
export const ABBREVIATED_MECHANISMS = ["spellchain", "necrocharge"] as const
export const isAbbreviatedMechanism = (
  value: PrintedMechanism,
): value is (typeof ABBREVIATED_MECHANISMS)[number] =>
  ABBREVIATED_MECHANISMS.some((code) => code === value)
export const isClassMechanism = (value: string): value is ClassMechanism =>
  CLASS_MECHANISMS.some((code) => code === value)
export const isCollabMechanism = (value: string): value is CollabMechanism =>
  COLLAB_MECHANISMS.some((code) => code === value)
export interface MechanismOptions {
  readonly coverage: "pending" | "unknown" | "partial" | "complete"
  readonly unknownCards: number
  readonly counts: Readonly<Record<string, number>>
}
