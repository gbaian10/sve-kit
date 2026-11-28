import {
  DECKLOG_ORIGINS,
  DECKLOG_PATH,
  type DeckLogCard,
  type DeckLogDeck,
  type DeckLogError,
  normalizeDeckLogCode,
  readDeckLogCards,
} from "../domain/decklog"
import type { Region } from "../domain/search"
import { BodyFormatError, readJsonBody } from "./body"

interface Limiter {
  limit(input: { key: string }): Promise<{ success: boolean }>
}

interface Statement {
  bind(...values: (string | null)[]): Statement
  first(): Promise<unknown>
  run(): Promise<unknown>
}

// Structural D1 binding: only the calls this endpoint makes.
interface Database {
  prepare(query: string): Statement
}

export interface DeckLogEnv {
  readonly DECKLOG_DB?: Database
  /** Every lookup, keyed by the caller's address: a person importing never comes near it. */
  readonly DECKLOG_LIMITER?: Limiter
  /** Lookups that reach DeckLog, one key per edition, so the Worker cannot hammer DeckLog. */
  readonly DECKLOG_UPSTREAM_LIMITER?: Limiter
}

const SVE_GAME_TITLE_ID = 6
const MAX_UPSTREAM_BYTES = 512 * 1024
// DeckLog's CloudFront answers scripted clients without a browser User-Agent with 403.
const USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36"
const FORMAT = /^[A-Z]{1,4}$/u
// eslint-disable-next-line no-control-regex -- Shown text must not carry control or direction marks.
const DISALLOWED_TEXT = /[\u0000-\u001f\u007f\p{Bidi_Control}]/u

const STATUS: Readonly<Record<DeckLogError, number>> = {
  "invalid-code": 400,
  "not-found": 404,
  "not-sve": 422,
  "rate-limited": 429,
  "upstream-unavailable": 502,
  "upstream-invalid": 502,
  "not-configured": 503,
  "store-unavailable": 503,
  "server-error": 500,
}

function failure(error: DeckLogError): Response {
  const status = STATUS[error]
  if (status >= 500) console.error(`decklog: ${error}`)
  return Response.json({ error }, { status, headers: { "Cache-Control": "no-store" } })
}

function success(deck: DeckLogDeck, stored: boolean): Response {
  // The deck behind a code never changes, so a stored one may stay in the browser for good. One
  // that could not be stored is not cached, so the next import asks again and stores it then.
  return Response.json(deck, {
    headers: {
      "Cache-Control": stored ? "private, max-age=31536000, immutable" : "no-store",
    },
  })
}

function shownText(value: unknown, pattern?: RegExp): string | null {
  if (typeof value !== "string") return null
  const text = value.trim()
  if (text === "" || text.length > 64 || DISALLOWED_TEXT.test(text)) return null
  return pattern === undefined || pattern.test(text) ? text : null
}

interface Sections {
  readonly leader: readonly DeckLogCard[]
  readonly main: readonly DeckLogCard[]
  readonly evolve: readonly DeckLogCard[]
}

function readSections(leader: unknown, main: unknown, evolve: unknown): Sections | undefined {
  const sections = {
    leader: readDeckLogCards(leader),
    main: readDeckLogCards(main),
    evolve: readDeckLogCards(evolve),
  }
  if (!sections.leader || !sections.main || !sections.evolve) return undefined
  return sections as Sections
}

// DeckLog leaves an empty leader or evolve section out or null; a deck always has a main deck.
const orEmpty = (value: unknown): unknown => value ?? []

type Lookup = DeckLogDeck | "not-found" | "not-sve" | "invalid"

/** DeckLog's answer reduced to the cards; no title, memo, comment or owner survives. */
function reduce(region: Region, code: string, value: unknown): Lookup {
  // An unknown code is answered with 200 and an empty array.
  if (Array.isArray(value) && value.length === 0) return "not-found"
  if (typeof value !== "object" || value === null || Array.isArray(value)) return "invalid"
  const record = value as Record<string, unknown>
  if (typeof record["game_title_id"] !== "number") return "invalid"
  if (record["game_title_id"] !== SVE_GAME_TITLE_ID) return "not-sve"
  const sections = readSections(
    orEmpty(record["p_list"]),
    record["list"],
    orEmpty(record["sub_list"]),
  )
  if (!sections || sections.main.length === 0) return "invalid"
  return {
    region,
    code,
    format: shownText(record["deck_param1"], FORMAT),
    className: shownText(record["deck_param2"]),
    ...sections,
  }
}

async function fetchDeckLog(region: Region, code: string): Promise<Lookup | "unavailable"> {
  const origin = DECKLOG_ORIGINS[region]
  // Every request DeckLog gets from us is logged before it is sent, so a deployment can show that
  // stored codes are not asked again (docs/deployment/cloudflare-development.md §12).
  console.warn(`decklog: upstream ${region}`)
  let response: Response
  try {
    // eslint-disable-next-line no-restricted-globals -- DeckLog allows no cross-origin reads; the Worker asks for the browser.
    response = await fetch(`${origin}/system/app/api/view/${code}`, {
      method: "POST",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
        "User-Agent": USER_AGENT,
        Origin: origin,
        Referer: `${origin}/view/${code}`,
      },
      body: "{}",
      redirect: "manual",
      signal: AbortSignal.timeout(10_000),
    })
  } catch {
    return "unavailable"
  }
  // DeckLog answers 200 for every code it knows, and for unknown ones too (with `[]`).
  if (response.status !== 200) {
    await response.body?.cancel().catch(() => undefined)
    return "unavailable"
  }
  try {
    return reduce(region, code, await readJsonBody(response.body, MAX_UPSTREAM_BYTES))
  } catch (error) {
    // A body that stalls past the timeout or breaks off is DeckLog not answering, not bad content.
    return error instanceof BodyFormatError ? "invalid" : "unavailable"
  }
}

/** A stored row in our own shape; `undefined` when it does not read back. */
function readRow(
  region: Region,
  code: string,
  row: Record<string, unknown>,
): DeckLogDeck | undefined {
  let cards: unknown
  try {
    cards = JSON.parse(typeof row["cards"] === "string" ? row["cards"] : "")
  } catch {
    return undefined
  }
  if (typeof cards !== "object" || cards === null || Array.isArray(cards)) return undefined
  const stored = cards as Record<string, unknown>
  const sections = readSections(stored["leader"], stored["main"], stored["evolve"])
  if (!sections || sections.main.length === 0) return undefined
  return {
    region,
    code,
    format: shownText(row["format"], FORMAT),
    className: shownText(row["class_name"]),
    ...sections,
  }
}

/**
 * The stored deck, or null when there is none. A row that does not read back is removed, so the
 * code is fetched once more and stored again; any D1 failure rejects.
 */
async function readStored(db: Database, region: Region, code: string): Promise<DeckLogDeck | null> {
  const select = () =>
    db
      .prepare(
        "SELECT format, class_name, cards FROM decklog_deck WHERE region = ?1 AND code = ?2 LIMIT 1",
      )
      .bind(region, code)
      .first()
  const row = await select()
  if (row === null) return null
  // D1 rows are untyped; they are checked here and never leave this module unchecked.
  const record = row as Record<string, unknown>
  const deck = readRow(region, code, record)
  if (deck) return deck
  console.error("decklog: unreadable-row")
  // Only the row read here goes: another lookup may already have replaced it with a good one, which
  // is then read back and served.
  await db
    .prepare("DELETE FROM decklog_deck WHERE region = ?1 AND code = ?2 AND cards = ?3")
    .bind(region, code, typeof record["cards"] === "string" ? record["cards"] : null)
    .run()
  const again = await select()
  return again === null ? null : (readRow(region, code, again as Record<string, unknown>) ?? null)
}

async function store(db: Database, deck: DeckLogDeck): Promise<void> {
  // Codes are immutable, so when two lookups of one code race, the first row stays.
  const cards = JSON.stringify({ leader: deck.leader, main: deck.main, evolve: deck.evolve })
  await db
    .prepare(
      `INSERT INTO decklog_deck (region, code, format, class_name, cards, fetched_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6)
       ON CONFLICT (region, code) DO NOTHING`,
    )
    .bind(deck.region, deck.code, deck.format, deck.className, cards, new Date().toISOString())
    .run()
}

/** `GET /api/decklog/{jp|en}/{code}`: the stored deck, else DeckLog's, stored for next time. */
export async function lookUpDeckLog(
  request: Request,
  env: DeckLogEnv,
  pathname: string,
): Promise<Response> {
  if (request.method !== "GET")
    return new Response(null, {
      status: 405,
      headers: { Allow: "GET", "Cache-Control": "no-store" },
    })
  const match = /^\/(jp|en)\/([^/]*)$/u.exec(pathname.slice(DECKLOG_PATH.length))
  if (!match) return new Response(null, { status: 404, headers: { "Cache-Control": "no-store" } })
  const region = match[1] as Region
  const code = match[2] === undefined ? undefined : normalizeDeckLogCode(match[2])
  if (code === undefined || code !== match[2]) return failure("invalid-code")

  const db = env.DECKLOG_DB
  const limiter = env.DECKLOG_LIMITER
  const upstream = env.DECKLOG_UPSTREAM_LIMITER
  if (!db || !limiter || !upstream) return failure("not-configured")

  try {
    const address = request.headers.get("CF-Connecting-IP") ?? "unknown"
    if (!(await limiter.limit({ key: `decklog:${address}` })).success)
      return failure("rate-limited")
    // A code is asked of DeckLog only when D1 says it has no row; a D1 failure is not that answer.
    let stored: DeckLogDeck | null
    try {
      stored = await readStored(db, region, code)
    } catch {
      return failure("store-unavailable")
    }
    if (stored) return success(stored, true)
    if (!(await upstream.limit({ key: `decklog-upstream:${region}` })).success)
      return failure("rate-limited")
    const result = await fetchDeckLog(region, code)
    if (result === "unavailable") return failure("upstream-unavailable")
    if (result === "invalid") return failure("upstream-invalid")
    if (result === "not-found" || result === "not-sve") return failure(result)
    const saved = await store(db, result).then(
      () => true,
      () => {
        console.error("decklog: store-write")
        return false
      },
    )
    return success(result, saved)
  } catch {
    return failure("server-error")
  }
}
