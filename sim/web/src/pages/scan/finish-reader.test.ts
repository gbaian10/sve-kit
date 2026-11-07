import { describe, expect, it, vi } from "vitest"

import { parseScanCards, type ScanCard } from "../../domain/scan/search"
import { FinishReader } from "./finish-reader"

const cards = parseScanCards(
  [
    { no: "BP01-001", c: "c1", n: 1, f: 0, nm: "a", a: "x", k: "elf" },
    { no: "BP01-SL01", c: "c1", n: 2, f: 0, nm: "a", a: "x", k: "elf" },
    { no: "BP01-002", c: "c2", n: 3, f: 0, nm: "b", a: "y", k: "elf" },
  ],
  3,
)
const [plain, , other] = cards
if (!plain || !other) throw new Error("fixture cards")
const grey = (_card: ScanCard) => Promise.resolve(new Uint8ClampedArray(120 * 168 * 4).fill(128))

describe("printing image reader", () => {
  it("only knows groups with more than one printing", () => {
    const reader = new FinishReader(cards, grey)
    expect(reader.group(plain)?.map((card) => card.no)).toEqual(["BP01-001", "BP01-SL01"])
    expect(reader.group(other)).toBeUndefined()
  })

  it("answers at once only after every image of the group is read, reading each image once", async () => {
    const load = vi.fn(grey)
    const reader = new FinishReader(cards, load)
    const group = reader.group(plain) ?? []
    expect(reader.ready(group)).toBeNull()
    expect(reader.ready(group)).toBeNull()
    expect(await reader.loaded(group)).toHaveLength(2)
    expect(reader.ready(group)).toHaveLength(2)
    expect(load).toHaveBeenCalledTimes(2)
    expect(load.mock.calls.map(([card]) => card.no)).toEqual(["BP01-001", "BP01-SL01"])
  })

  it("gives up on a group whose image cannot be read, without retrying it every frame", async () => {
    const load = vi.fn((card: ScanCard) =>
      card.imageId === 2 ? Promise.reject(new Error("404")) : grey(card),
    )
    const reader = new FinishReader(cards, load)
    const group = reader.group(plain) ?? []
    expect(await reader.loaded(group)).toBeNull()
    expect(reader.ready(group)).toBeNull()
    expect(load).toHaveBeenCalledTimes(2)
  })
})
