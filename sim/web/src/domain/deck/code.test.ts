import { describe, expect, it } from "vitest"

import {
  DeckCodeError,
  deckCodeOf,
  type DeckCodePayload,
  decodeDeckCode,
  encodeDeckCode,
} from "./code"

const full: DeckCodePayload = {
  edition: "jp",
  basis: { kind: "class", classCode: "elf" },
  leader: 20_001,
  main: [
    { intId: 20_105, count: 3 },
    { intId: 20_102, count: 2 },
    { intId: 61_000, count: 1 },
  ],
  evolve: [{ intId: 20_200, count: 3 }],
  side: [{ intId: 20_300, count: 1 }],
  name: "精靈 快攻 BP19",
}

/** A code by hand: header for a JP class deck, then one main card with the given varint bytes. */
function codeWithMain(cardBytes: number[]): string {
  const bytes = [1, 0, 0, 1, 0x61, 0, 1, ...cardBytes, 0, 0]
  return `SVEK-${btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "")}`
}
// 2^53 + 1 as a varint: 0x81, six 0x80 continuation bytes, then 0x10 (16 × 2^49).
const unsafeCode = () => codeWithMain([0x81, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x10, 1])
function overflowingIds(): string {
  const big = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x0f]
  const bytes = [1, 0, 0, 1, 0x61, 0, 2, ...big, 1, ...big, 1, 0, 0]
  return `SVEK-${btoa(String.fromCharCode(...bytes))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "")}`
}

describe("deck code", () => {
  it("round-trips a deck with every section, sorting cards by id", () => {
    const code = encodeDeckCode(full)
    expect(code).toMatch(/^SVEK-[A-Za-z0-9_-]+$/u)
    expect(decodeDeckCode(code)).toEqual({
      ...full,
      main: [
        { intId: 20_102, count: 2 },
        { intId: 20_105, count: 3 },
        { intId: 61_000, count: 1 },
      ],
    })
  })

  it("round-trips an EN title deck without a leader, side cards or a name", () => {
    const payload: DeckCodePayload = {
      edition: "en",
      basis: { kind: "title", titleCode: "umamusume_pretty_derby" },
      leader: null,
      main: [{ intId: 60_001, count: 3 }],
      evolve: [],
      side: [],
      name: "",
    }
    expect(decodeDeckCode(encodeDeckCode(payload))).toEqual(payload)
  })

  it("keeps a forty-card deck short", () => {
    const main = Array.from({ length: 14 }, (_, i) => ({ intId: 20_100 + i * 2, count: 3 }))
    expect(encodeDeckCode({ ...full, main, name: "" }).length).toBeLessThan(90)
  })

  it("takes a whole input that is a code or a link, nothing longer", () => {
    const code = encodeDeckCode(full)
    expect(deckCodeOf(`https://example.test/decks/import?d=${code}`)).toBe(code)
    expect(deckCodeOf(`  ${code}\n`)).toBe(code)
    // A deck list stays a list even when a line looks like a code.
    expect(deckCodeOf(`# ${code}\n3 BP01-001`)).toBeUndefined()
    expect(deckCodeOf("3 BP01-001")).toBeUndefined()
  })

  it.each([
    ["no prefix", "ABC"],
    ["not base64url", "SVEK-***"],
    ["unknown version", `SVEK-${btoa("\u0002\u0000\u0000").replace(/=+$/u, "")}`],
    ["truncated", encodeDeckCode(full).slice(0, 12)],
    ["an integer beyond the safe range", unsafeCode()],
    ["ids that add up beyond the safe range", overflowingIds()],
  ])("refuses a code with %s", (_label, code) => {
    expect(() => decodeDeckCode(code)).toThrow(DeckCodeError)
  })
})
