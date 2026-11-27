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
  "server-error": 500,
}

function failure(error: DeckLogError): Response {
  const status = STATUS[error]
  if (status >= 500) console.error(`decklog: ${error}`)
  return Response.json({ error }, { status, headers: { "Cache-Control": "no-store" } })
}

function success(deck: DeckLogDeck): Response {
  // The deck behind a code never changes; the browser may keep it as long as it likes.
  return Response.json(deck, {
    headers: { "Cache-Control": "private, max-age=31536000, immutable" },
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

type Lookup = DeckLogDeck | "not-found" | "not-sve" | "invalid"

/** DeckLog's answer reduced to the cards; no title, memo, comment or owner survives. */
function reduce(region: Region, code: string, value: unknown): Lookup {
  // An unknown code is answered with 200 and an empty array.
  if (Array.isArray(value) && value.length === 0) return "not-found"
  if (typeof value !== "object" || value === null || Array.isArray(value)) return "invalid"
  const record = value as Record<string, unknown>
  if (typeof record["game_title_id"] !== "number") return "invalid"
  if (record["game_title_id"] !== SVE_GAME_TITLE_ID) return "not-sve"
  const sections = readSections(record["p_list"], record["list"], record["sub_list"])
  if (!sections) return "invalid"
  return {
    region,
    code,
    format: shownText(record["deck_param1"], FORMAT),
    className: shownText(record["deck_param2"]),
    ...sections,
  }
}

async function readLimited(response: Response): Promise<unknown> {
  const reader = response.body?.getReader()
  if (!reader) throw new Error("Empty DeckLog answer")
  const chunks: Uint8Array[] = []
  let size = 0
  for (;;) {
    const { value, done } = await reader.read()
    if (done) break
    size += value.byteLength
    if (size > MAX_UPSTREAM_BYTES) {
      await reader.cancel()
      throw new Error("DeckLog answer too large")
    }
    chunks.push(value)
  }
  const bytes = new Uint8Array(size)
  let offset = 0
  for (const chunk of chunks) {
    bytes.set(chunk, offset)
    offset += chunk.byteLength
  }
  return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes))
}

async function fetchDeckLog(region: Region, code: string): Promise<Lookup | "unavailable"> {
  const origin = DECKLOG_ORIGINS[region]
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
  if (!response.ok) {
    await response.body?.cancel().catch(() => undefined)
    return "unavailable"
  }
  try {
    return reduce(region, code, await readLimited(response))
  } catch {
    return "invalid"
  }
}

async function readStored(db: Database, region: Region, code: string): Promise<DeckLogDeck | null> {
  const row = await db
    .prepare(
      "SELECT format, class_name, cards FROM decklog_deck WHERE region = ?1 AND code = ?2 LIMIT 1",
    )
    .bind(region, code)
    .first()
  if (row === null) return null
  // D1 rows are untyped; a row that does not read back is treated as missing and fetched again.
  const record = row as Record<string, unknown>
  let cards: unknown
  try {
    cards = JSON.parse(typeof record["cards"] === "string" ? record["cards"] : "")
  } catch {
    return null
  }
  if (typeof cards !== "object" || cards === null) return null
  const stored = cards as Record<string, unknown>
  const sections = readSections(stored["leader"], stored["main"], stored["evolve"])
  if (!sections) return null
  return {
    region,
    code,
    format: shownText(record["format"], FORMAT),
    className: shownText(record["class_name"]),
    ...sections,
  }
}

async function store(db: Database, deck: DeckLogDeck): Promise<void> {
  // A row is replaced only when it no longer read back (see readStored).
  const cards = JSON.stringify({ leader: deck.leader, main: deck.main, evolve: deck.evolve })
  await db
    .prepare(
      `INSERT INTO decklog_deck (region, code, format, class_name, cards, fetched_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6)
       ON CONFLICT (region, code) DO UPDATE SET
         format = excluded.format, class_name = excluded.class_name, cards = excluded.cards,
         fetched_at = excluded.fetched_at`,
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
    const stored = await readStored(db, region, code).catch(() => {
      // An unreadable store only costs a DeckLog request, still bounded by the upstream limit.
      console.error("decklog: store-read")
      return null
    })
    if (stored) return success(stored)
    if (!(await upstream.limit({ key: `decklog-upstream:${region}` })).success)
      return failure("rate-limited")
    const result = await fetchDeckLog(region, code)
    if (result === "unavailable") return failure("upstream-unavailable")
    if (result === "invalid") return failure("upstream-invalid")
    if (result === "not-found" || result === "not-sve") return failure(result)
    await store(db, result).catch(() => {
      console.error("decklog: store-write")
    })
    return success(result)
  } catch {
    return failure("server-error")
  }
}
