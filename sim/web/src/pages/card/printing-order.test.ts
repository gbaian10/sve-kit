import { describe, expect, it } from "vitest"

import { type Row } from "../../data"
import { printingGroups } from "./printing-order"

const printing = (cardNo: string, rarity: string | null, region = "jp", premium = false): Row => ({
  id: `${region}:${cardNo}`,
  card_no: cardNo,
  rarity_code: rarity?.toLowerCase() ?? null,
  region,
  premium,
})

describe("printing rows", () => {
  it("separates editions and orders Merlin's ordinary, reprint, special and promotional editions", () => {
    const rows = [
      printing("PR-299", "PR"),
      printing("BP01-SL09", "SL"),
      printing("BP01-053EN", "LG", "en"),
      printing("EBD02-005", "LG"),
      printing("BP01-053", "LG"),
    ]
    expect(
      printingGroups(rows).map(({ region, printings }) => [
        region,
        printings.map((row) => row["card_no"]),
      ]),
    ).toEqual([
      ["jp", ["BP01-053", "EBD02-005", "BP01-SL09", "PR-299"]],
      ["en", ["BP01-053EN"]],
    ])
    expect(rows[0]?.["card_no"]).toBe("PR-299")
  })
  it("puts premium variants after their base tier, unknowns before promos, and sorts tied card numbers", () => {
    const rows = [
      printing("PR-1", "PR"),
      printing("Z-1", null),
      printing("A-1", "LG", "jp", true),
      printing("Z-1", "LG"),
      printing("A-2", "BR"),
      printing("A-10", "BR"),
      printing("A-1", "SR"),
      printing("A-1", "GR"),
      printing("A-1", "SL"),
      printing("A-1", "UR"),
      printing("A-1", "SP"),
      printing("A-1", "SSP"),
      printing("A-1", "PREMIUM", "jp", true),
    ]
    expect(
      printingGroups(rows)[0]?.printings.map((row) => [row["card_no"], row["rarity_code"]]),
    ).toEqual([
      ["A-2", "br"],
      ["A-10", "br"],
      ["A-1", "sr"],
      ["A-1", "gr"],
      ["Z-1", "lg"],
      ["A-1", "lg"],
      ["A-1", "sl"],
      ["A-1", "ur"],
      ["A-1", "sp"],
      ["A-1", "ssp"],
      ["A-1", "premium"],
      ["Z-1", null],
      ["PR-1", "pr"],
    ])
  })
})
