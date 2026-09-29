import { NEUTRAL_CLASS } from "./query/model"

// The design names classes by their digital-game craft (tokens.css); the snapshot uses the SVE
// vocabulary codes. Codes without a design colour or icon (Portalcraft / nemesis today) render in
// the neutral style until the design adds them.
export type DesignClass = "forest" | "sword" | "rune" | "dragon" | "abyss" | "haven" | "neutral"

const STYLE: Record<string, DesignClass> = {
  elf: "forest",
  royal: "sword",
  witch: "rune",
  dragon: "dragon",
  nightmare: "abyss",
  bishop: "haven",
}

/** The design's colour/icon family for a vocabulary class code (null = neutral card). */
export function classStyle(code: string | null): DesignClass {
  return code === null ? "neutral" : (STYLE[code] ?? "neutral")
}

// The game's own class order (rulebook and card list), then anything newer, then neutral.
const CLASS_ORDER = ["elf", "royal", "witch", "dragon", "nightmare", "bishop", "nemesis"]

/** Quick-bar order: known classes in game order, unknown codes after them, neutral last. */
export function quickBarClasses(codes: readonly string[]): string[] {
  const known = CLASS_ORDER.filter((code) => codes.includes(code))
  const rest = codes.filter((code) => !CLASS_ORDER.includes(code) && code !== NEUTRAL_CLASS).sort()
  return [...known, ...rest, NEUTRAL_CLASS]
}
