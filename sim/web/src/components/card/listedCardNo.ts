import type { CardSummary } from "../../data"

/**
 * What a list cell shows where the card number goes: unlisted printings have no number of their
 * own, so they show the code printed on the card face, or nothing when it has none.
 */
export function listedCardNo(
  summary: Pick<CardSummary, "cardNo" | "unlisted" | "printedCode">,
): string {
  return summary.unlisted === true ? (summary.printedCode ?? "") : summary.cardNo
}
