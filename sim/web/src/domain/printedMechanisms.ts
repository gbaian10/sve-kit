export const CLASS_MECHANISMS = [
  "combo",
  "spellchain",
  "stack",
  "earth_rite",
  "overflow",
  "necrocharge",
  "sanguine",
] as const
export const COLLAB_MECHANISMS = [
  "serve",
  "lesson",
  "starting_amulet",
  "single_drive",
  "twin_drive",
] as const
export type PrintedMechanism =
  (typeof CLASS_MECHANISMS)[number] | (typeof COLLAB_MECHANISMS)[number]
export const isClassMechanism = (value: string): value is (typeof CLASS_MECHANISMS)[number] =>
  CLASS_MECHANISMS.some((code) => code === value)
export const isCollabMechanism = (value: string): value is (typeof COLLAB_MECHANISMS)[number] =>
  COLLAB_MECHANISMS.some((code) => code === value)
export interface MechanismOptions {
  readonly coverage: "pending" | "unknown" | "partial" | "complete"
  readonly unknownCards: number
  readonly counts: Readonly<Record<string, number>>
}
