import {
  DECKLOG_ERRORS,
  DECKLOG_PATH,
  type DeckLogDeck,
  type DeckLogError,
  readDeckLogCards,
} from "../domain/decklog"
import type { Region } from "../domain/search"

export type DeckLogResult =
  { readonly deck: DeckLogDeck } | { readonly error: DeckLogError | "offline" }

function errorOf(body: unknown): DeckLogError | undefined {
  const error = (body as { error?: unknown } | null)?.error
  return DECKLOG_ERRORS.find((known) => known === error)
}

function deckOf(body: unknown, region: Region, code: string): DeckLogDeck | undefined {
  if (typeof body !== "object" || body === null) return undefined
  const record = body as Record<string, unknown>
  const leader = readDeckLogCards(record["leader"])
  const main = readDeckLogCards(record["main"])
  const evolve = readDeckLogCards(record["evolve"])
  const text = (value: unknown) => (typeof value === "string" ? value : null)
  if (record["region"] !== region || record["code"] !== code || !leader || !main || !evolve)
    return undefined
  return {
    region,
    code,
    format: text(record["format"]),
    className: text(record["className"]),
    leader,
    main,
    evolve,
  }
}

/** Asks our Worker for a DeckLog deck; it answers from its store or from DeckLog. */
export async function fetchDeckLogDeck(
  region: Region,
  code: string,
  signal?: AbortSignal,
): Promise<DeckLogResult> {
  let response: Response
  try {
    response = await fetch(`${DECKLOG_PATH}/${region}/${code}`, {
      headers: { Accept: "application/json" },
      redirect: "error",
      signal: signal
        ? AbortSignal.any([signal, AbortSignal.timeout(20_000)])
        : AbortSignal.timeout(20_000),
    })
  } catch (error) {
    if (signal?.aborted) throw error
    return { error: "offline" }
  }
  let body: unknown
  try {
    body = await response.json()
  } catch {
    return { error: response.ok ? "upstream-invalid" : "server-error" }
  }
  if (response.ok) {
    const deck = deckOf(body, region, code)
    return deck ? { deck } : { error: "upstream-invalid" }
  }
  return { error: errorOf(body) ?? "server-error" }
}
