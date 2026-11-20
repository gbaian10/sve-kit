import { CARD_REPORT_PATH, type CardReportInput } from "../domain/card-report"

export type CardReportResult =
  | { readonly issueNumber: number }
  | { readonly error: "verification" | "rate" | "invalid" | "unavailable" | "server" }

export async function sendCardReport(report: CardReportInput): Promise<CardReportResult> {
  try {
    const response = await fetch(CARD_REPORT_PATH, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(report),
      redirect: "error",
      signal: AbortSignal.timeout(35_000),
    })
    switch (response.status) {
      case 400:
      case 415:
        return { error: "invalid" }
      case 403:
        return { error: "verification" }
      case 429:
        return { error: "rate" }
      case 503:
        return { error: "unavailable" }
      case 201: {
        const body: unknown = await response.json()
        const number = (body as { issueNumber?: unknown } | null)?.issueNumber
        if (typeof number === "number" && Number.isSafeInteger(number) && number > 0) {
          return { issueNumber: number }
        }
        break
      }
    }
  } catch {
    return { error: "server" }
  }
  return { error: "server" }
}
