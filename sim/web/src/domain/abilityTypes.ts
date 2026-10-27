export const ABILITY_TYPES = [
  "quick",
  "fanfare",
  "last_words",
  "on_evolve",
  "on_attack",
  "advanced_act",
  "on_super_evolve",
  "act",
  "fusion",
] as const
export type AbilityType = (typeof ABILITY_TYPES)[number]
export const isAbilityType = (value: string): value is AbilityType =>
  ABILITY_TYPES.some((code) => code === value)
