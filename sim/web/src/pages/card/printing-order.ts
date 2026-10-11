import { type Row, stringValue } from "../../data"
import type { Region } from "../../domain/search"

// A bare premium has no rarity code, so it and the other uncoded printings get sort keys of their own.
const RARITY_ORDER = ["br", "sr", "gr", "lg", "sl", "ur", "sp", "ssp", "premium", "unknown", "pr"]
const rank = (row: Row): number => {
  const code = row["rarity_code"]
  const key = typeof code === "string" ? code : row["premium"] === true ? "premium" : "unknown"
  const index = RARITY_ORDER.indexOf(key)
  return index < 0 ? RARITY_ORDER.indexOf("unknown") : index
}

export function printingGroups(rows: readonly Row[]): { region: Region; printings: Row[] }[] {
  return (["jp", "en"] as const)
    .map((region) => ({
      region,
      printings: rows
        .filter((row) => row["region"] === region)
        .sort(
          (a, b) =>
            rank(a) - rank(b) ||
            Number(a["premium"] === true) - Number(b["premium"] === true) ||
            stringValue(a["card_no"]).localeCompare(stringValue(b["card_no"]), "en", {
              numeric: true,
            }) ||
            stringValue(a["id"]).localeCompare(stringValue(b["id"]), "en"),
        ),
    }))
    .filter((group) => group.printings.length > 0)
}
