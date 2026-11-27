// @vitest-environment node
import { readdirSync, readFileSync } from "node:fs"
import path from "node:path"

import { afterAll, afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest"
import { getPlatformProxy, unstable_splitSqlQuery } from "wrangler"

import worker from "../src/cloudflare/worker"

// The local D1 and rate limiters of wrangler.dev.jsonc (Miniflare, in memory); DeckLog itself is a
// stubbed fetch answering synthetic decks.
type Proxy = Awaited<ReturnType<typeof getPlatformProxy<Record<string, unknown>>>>
type Env = Parameters<typeof worker.fetch>[1]
type Database = NonNullable<Env["DECKLOG_DB"]>
type Limiter = NonNullable<Env["DECKLOG_LIMITER"]>

const root = path.resolve(import.meta.dirname, "..")
let proxy: Proxy
let db: Database

beforeAll(async () => {
  proxy = await getPlatformProxy({
    configPath: path.join(root, "wrangler.dev.jsonc"),
    persist: false,
    remoteBindings: false,
    envFiles: [],
  })
  db = proxy.env["DECKLOG_DB"] as Database
  // Statements split as `wrangler d1 migrations apply` splits them.
  const migrations = path.join(root, "migrations")
  for (const file of readdirSync(migrations).sort())
    for (const statement of unstable_splitSqlQuery(
      readFileSync(path.join(migrations, file), "utf8"),
    ))
      await db.prepare(statement).run()
}, 60_000)

afterAll(async () => {
  await proxy.dispose()
})

// Synthetic answers in DeckLog's shape: the extra fields stand for what must not be stored.
function deckLogAnswer(overrides: Record<string, unknown> = {}) {
  return {
    id: 1,
    deck_id: "AB12C",
    title: "Someone's deck",
    memo: "private memo",
    comment: "comment",
    game_title_id: 6,
    deck_param1: "N",
    deck_param2: "ナイトメア",
    p_list: [{ card_number: "BP01-LD01", num: 1, name: "Leader", img: "x.png" }],
    list: [
      { card_number: "BP01-001", num: 3, name: "A" },
      { card_number: "BP01-SL01", num: 1, name: "A (SL)" },
    ],
    sub_list: [{ card_number: "BP01-P02", num: 2, name: "B" }],
    ...overrides,
  }
}

const deck = {
  region: "jp",
  code: "AB12C",
  format: "N",
  className: "ナイトメア",
  leader: [{ cardNo: "BP01-LD01", count: 1 }],
  main: [
    { cardNo: "BP01-001", count: 3 },
    { cardNo: "BP01-SL01", count: 1 },
  ],
  evolve: [{ cardNo: "BP01-P02", count: 2 }],
}

let address = 0
let upstream: ReturnType<typeof vi.fn<typeof fetch>>

beforeEach(async () => {
  await db.prepare("DELETE FROM decklog_deck").run()
  upstream = vi.fn<typeof fetch>()
  vi.stubGlobal("fetch", upstream)
  vi.spyOn(console, "error").mockImplementation(() => undefined)
  vi.spyOn(console, "warn").mockImplementation(() => undefined)
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

const passing: Limiter = { limit: () => Promise.resolve({ success: true }) }

/**
 * The test bindings: local D1 and the real per-address limiter; the shared upstream limiter passes
 * unless a test gives its own. An override of `undefined` leaves that binding out.
 */
function environment(overrides: { [K in keyof Env]?: Env[K] | undefined } = {}): Env {
  const env: Record<string, unknown> = {
    PREVIEW_BUCKET: {
      get: () => Promise.reject(new Error("Unexpected bucket access")),
      head: () => Promise.reject(new Error("Unexpected bucket access")),
    },
    ASSETS: { fetch: () => Promise.reject(new Error("Unexpected asset request")) },
    DECKLOG_DB: db,
    DECKLOG_LIMITER: proxy.env["DECKLOG_LIMITER"],
    DECKLOG_UPSTREAM_LIMITER: passing,
    ...overrides,
  }
  for (const [name, value] of Object.entries(env))
    if (value === undefined) Reflect.deleteProperty(env, name)
  return env as unknown as Env
}

/** Each client asks from its own address, so the real per-address limiter starts fresh. */
function client(env: Env = environment()) {
  address += 1
  const ip = `192.0.2.${String(address)}`
  return (target: string, init: RequestInit = {}) =>
    worker.fetch(
      new Request(`https://dev.svekit.app/api/decklog/${target}`, {
        ...init,
        headers: { "CF-Connecting-IP": ip },
      }),
      env,
      { waitUntil: () => undefined },
    )
}

function stored(region = "jp", code = "AB12C") {
  return db
    .prepare("SELECT * FROM decklog_deck WHERE region = ?1 AND code = ?2")
    .bind(region, code)
    .first()
}

const storedCards = JSON.stringify({ leader: deck.leader, main: deck.main, evolve: deck.evolve })

it("fetches a missing deck from DeckLog once, keeps only its cards, then serves it from D1", async () => {
  upstream.mockResolvedValueOnce(Response.json(deckLogAnswer()))
  const get = client()

  const first = await get("jp/AB12C")

  expect(first.status).toBe(200)
  expect(first.headers.get("Cache-Control")).toBe("private, max-age=31536000, immutable")
  expect(await first.json()).toEqual(deck)
  expect(upstream).toHaveBeenCalledOnce()
  const [url, init] = upstream.mock.calls[0] ?? []
  expect(url).toBe("https://decklog.bushiroad.com/system/app/api/view/AB12C")
  expect(init?.method).toBe("POST")
  expect(init?.body).toBe("{}")
  const headers = new Headers(init?.headers)
  expect(headers.get("Referer")).toBe("https://decklog.bushiroad.com/view/AB12C")
  expect(headers.get("User-Agent")).toMatch(/^Mozilla\/5\.0 /u)

  const row = (await stored()) as Record<string, unknown>
  expect(row).toEqual({
    region: "jp",
    code: "AB12C",
    format: "N",
    class_name: "ナイトメア",
    cards: storedCards,
    fetched_at: expect.stringMatching(/^\d{4}-\d{2}-\d{2}T/u) as unknown,
  })
  expect(JSON.stringify(row)).not.toMatch(/Someone|memo|comment|img/u)

  const second = await get("jp/AB12C")
  expect(second.status).toBe(200)
  expect(await second.json()).toEqual(deck)
  expect(upstream).toHaveBeenCalledOnce()
})

it("asks English DeckLog for English codes and keeps the editions apart", async () => {
  upstream.mockResolvedValueOnce(
    Response.json(
      deckLogAnswer({
        deck_param2: "Runecraft",
        p_list: [{ card_number: "SDD03-LD01EN", num: 1 }],
        list: [{ card_number: "BP09-R047EN", num: 3 }],
        sub_list: null,
      }),
    ),
  )
  const get = client()

  const response = await get("en/AB12C")

  expect(response.status).toBe(200)
  expect(await response.json()).toEqual({
    region: "en",
    code: "AB12C",
    format: "N",
    className: "Runecraft",
    leader: [{ cardNo: "SDD03-LD01EN", count: 1 }],
    main: [{ cardNo: "BP09-R047EN", count: 3 }],
    evolve: [],
  })
  expect(upstream.mock.calls[0]?.[0]).toBe(
    "https://decklog-en.bushiroad.com/system/app/api/view/AB12C",
  )

  // The same code in the Japanese system is another deck.
  upstream.mockResolvedValueOnce(Response.json(deckLogAnswer()))
  expect(await (await get("jp/AB12C")).json()).toEqual(deck)
  expect(upstream).toHaveBeenCalledTimes(2)
})

it("rejects malformed codes and paths without asking anyone", async () => {
  const limit = vi.fn()
  const get = client(environment({ DECKLOG_LIMITER: { limit } }))

  for (const target of ["jp/ab12c", "jp/AB1", "jp/AB12C-1", "jp/%41B12C", "jp/ABCDEFGHIJK"]) {
    const response = await get(target)
    expect(response.status, target).toBe(400)
    expect(await response.json()).toEqual({ error: "invalid-code" })
  }
  expect((await get("kr/AB12C")).status).toBe(404)
  expect((await get("jp/AB12C/extra")).status).toBe(404)
  const post = await get("jp/AB12C", { method: "POST" })
  expect(post.status).toBe(405)
  expect(post.headers.get("Allow")).toBe("GET")
  expect(limit).not.toHaveBeenCalled()
  expect(upstream).not.toHaveBeenCalled()
})

it("says when a code does not exist or is not a Shadowverse: EVOLVE deck, and stores neither", async () => {
  const get = client()
  upstream.mockResolvedValueOnce(Response.json([]))
  const missing = await get("jp/AB12C")
  expect(missing.status).toBe(404)
  expect(missing.headers.get("Cache-Control")).toBe("no-store")
  expect(await missing.json()).toEqual({ error: "not-found" })

  upstream.mockResolvedValueOnce(Response.json(deckLogAnswer({ game_title_id: 2 })))
  const other = await get("jp/AB12C")
  expect(other.status).toBe(422)
  expect(await other.json()).toEqual({ error: "not-sve" })

  expect(await stored()).toBeNull()
  expect(upstream).toHaveBeenCalledTimes(2)
})

it("tells an unreachable DeckLog from one that answers something unexpected", async () => {
  // More attempts than one address may make, so each comes from its own.
  const get = (target: string) => client()(target)
  const stalled = () =>
    new Response(
      new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('{"game_title_id":6,'))
          controller.error(new DOMException("The operation timed out.", "TimeoutError"))
        },
      }),
    )
  const unavailable = [
    () => Promise.reject(new TypeError("network")),
    () => Promise.resolve(new Response("busy", { status: 503 })),
    () => Promise.resolve(new Response(null, { status: 302, headers: { Location: "/" } })),
    () => Promise.resolve(Response.json(deckLogAnswer(), { status: 201 })),
    () => Promise.resolve(stalled()),
  ]
  for (const answer of unavailable) {
    upstream.mockImplementationOnce(answer)
    const response = await get("jp/AB12C")
    expect(response.status).toBe(502)
    expect(await response.json()).toEqual({ error: "upstream-unavailable" })
  }

  const invalid = [
    new Response("<html>", { status: 200 }),
    Response.json({ game_title_id: 6 }),
    Response.json(deckLogAnswer({ list: [] })),
    Response.json(deckLogAnswer({ list: null })),
    Response.json(deckLogAnswer({ list: [{ card_number: "BP01-001", num: 0 }] })),
    Response.json(deckLogAnswer({ list: [{ card_number: "<script>", num: 1 }] })),
    Response.json(deckLogAnswer({ sub_list: "BP01-001" })),
    new Response("x".repeat(600 * 1024)),
  ]
  for (const answer of invalid) {
    upstream.mockResolvedValueOnce(answer)
    const response = await get("jp/AB12C")
    expect(response.status).toBe(502)
    expect(await response.json()).toEqual({ error: "upstream-invalid" })
  }
  expect(await stored()).toBeNull()
})

it("drops display text it cannot show safely", async () => {
  upstream.mockResolvedValueOnce(
    Response.json(deckLogAnswer({ deck_param1: "standard", deck_param2: "Rune‮craft" })),
  )
  const body = (await (await client()("jp/AB12C")).json()) as Record<string, unknown>
  expect(body["format"]).toBeNull()
  expect(body["className"]).toBeNull()
})

it("limits each address, stored decks included", async () => {
  upstream.mockResolvedValueOnce(Response.json(deckLogAnswer()))
  const get = client()
  const statuses: number[] = []
  for (let attempt = 0; attempt < 11; attempt += 1) statuses.push((await get("jp/AB12C")).status)
  expect(statuses).toEqual([...Array<number>(10).fill(200), 429])
  expect(upstream).toHaveBeenCalledOnce()
  // Another address is not affected.
  expect((await client()("jp/AB12C")).status).toBe(200)
})

it("caps DeckLog requests per edition, from any address, but still serves stored decks", async () => {
  const env = environment({
    DECKLOG_UPSTREAM_LIMITER: proxy.env["DECKLOG_UPSTREAM_LIMITER"] as Limiter,
  })
  upstream.mockImplementation(() => Promise.resolve(Response.json([])))
  // 30 misses in a minute from 30 addresses spend the English allowance.
  for (let attempt = 0; attempt < 30; attempt += 1)
    expect((await client(env)("en/ZZ99Z")).status).toBe(404)
  const limited = await client(env)("en/ZZ99Z")
  expect(limited.status).toBe(429)
  expect(await limited.json()).toEqual({ error: "rate-limited" })
  expect(upstream).toHaveBeenCalledTimes(30)

  // The Japanese allowance is separate, and a stored English deck needs no DeckLog request.
  await db
    .prepare(
      "INSERT INTO decklog_deck (region, code, format, class_name, cards, fetched_at) VALUES ('en', 'AB12C', 'N', NULL, ?1, 'x')",
    )
    .bind(storedCards)
    .run()
  expect((await client(env)("en/AB12C")).status).toBe(200)
  upstream.mockResolvedValueOnce(Response.json(deckLogAnswer()))
  expect((await client(env)("jp/AB12C")).status).toBe(200)
  expect(upstream).toHaveBeenCalledTimes(31)
})

it("fetches again when a stored row no longer reads back, and replaces it", async () => {
  for (const cards of ['{"main":7}', "[]", '{"leader":[],"main":[],"evolve":[]}']) {
    await db.prepare("DELETE FROM decklog_deck").run()
    await db
      .prepare(
        "INSERT INTO decklog_deck (region, code, format, class_name, cards, fetched_at) VALUES ('jp', 'AB12C', 'N', NULL, ?1, 'x')",
      )
      .bind(cards)
      .run()
    upstream.mockResolvedValueOnce(Response.json(deckLogAnswer()))

    const response = await client()("jp/AB12C")

    expect(await response.json(), cards).toEqual(deck)
    expect(((await stored()) as Record<string, unknown>)["cards"]).toBe(storedCards)
  }
  expect(upstream).toHaveBeenCalledTimes(3)
})

it("keeps the first stored row when two lookups of a code miss at once", async () => {
  const answers: ((response: Response) => void)[] = []
  upstream.mockImplementation(
    () =>
      new Promise<Response>((resolve) => {
        answers.push(resolve)
      }),
  )
  // The second lookup starts once the first is waiting on DeckLog, so both have missed D1.
  const first = client()("jp/AB12C")
  await vi.waitFor(() => {
    expect(answers).toHaveLength(1)
  })
  const second = client()("jp/AB12C")
  await vi.waitFor(() => {
    expect(answers).toHaveLength(2)
  })
  answers[0]?.(Response.json(deckLogAnswer()))
  expect((await first).status).toBe(200)
  // A different answer arriving second (DeckLog never sends one) must not replace the row.
  answers[1]?.(Response.json(deckLogAnswer({ sub_list: [] })))
  expect((await second).status).toBe(200)
  expect(((await stored()) as Record<string, unknown>)["cards"]).toBe(storedCards)
})

it("never asks DeckLog while D1 cannot be read, and does not let the browser keep an unsaved deck", async () => {
  const unreadable = await client(
    environment({
      DECKLOG_DB: {
        prepare: () => {
          throw new Error("D1 down")
        },
      },
    }),
  )("jp/AB12C")
  expect(unreadable.status).toBe(503)
  expect(await unreadable.json()).toEqual({ error: "store-unavailable" })
  expect(upstream).not.toHaveBeenCalled()

  // Reads work, writes fail: the deck is answered but not cached, so a later import stores it.
  type Statement = ReturnType<Database["prepare"]>
  const failingWrites = (statement: Statement): Statement => ({
    bind: (...values) => failingWrites(statement.bind(...values)),
    first: () => statement.first(),
    run: () => Promise.reject(new Error("D1 down")),
  })
  const readOnly: Database = { prepare: (query) => failingWrites(db.prepare(query)) }
  upstream.mockResolvedValueOnce(Response.json(deckLogAnswer()))
  const unsaved = await client(environment({ DECKLOG_DB: readOnly }))("jp/AB12C")
  expect(unsaved.status).toBe(200)
  expect(unsaved.headers.get("Cache-Control")).toBe("no-store")
  expect(await unsaved.json()).toEqual(deck)
  expect(await stored()).toBeNull()
})

it("refuses to run without its bindings", async () => {
  for (const missing of ["DECKLOG_DB", "DECKLOG_LIMITER", "DECKLOG_UPSTREAM_LIMITER"] as const) {
    const unconfigured = await client(environment({ [missing]: undefined }))("jp/AB12C")
    expect(unconfigured.status).toBe(503)
    expect(await unconfigured.json()).toEqual({ error: "not-configured" })
  }

  const failing = await client(
    environment({ DECKLOG_LIMITER: { limit: () => Promise.reject(new Error("limiter")) } }),
  )("jp/AB12C")
  expect(failing.status).toBe(500)
  expect(await failing.json()).toEqual({ error: "server-error" })
  expect(upstream).not.toHaveBeenCalled()
})

it("leaves a row another lookup has just repaired", async () => {
  await db
    .prepare(
      "INSERT INTO decklog_deck (region, code, format, class_name, cards, fetched_at) VALUES ('jp', 'AB12C', 'N', NULL, '[]', 'x')",
    )
    .run()
  // This lookup reads the unreadable row; before it removes it, another lookup puts a good one back.
  const racing: Database = {
    prepare: (query) => {
      if (query.startsWith("DELETE"))
        return {
          bind: (...values) => ({
            first: () => Promise.reject(new Error("unused")),
            bind: () => {
              throw new Error("unused")
            },
            run: async () => {
              await db
                .prepare(
                  "UPDATE decklog_deck SET cards = ?1 WHERE region = 'jp' AND code = 'AB12C'",
                )
                .bind(storedCards)
                .run()
              return db
                .prepare(query)
                .bind(...values)
                .run()
            },
          }),
          first: () => Promise.reject(new Error("unused")),
          run: () => Promise.reject(new Error("unused")),
        }
      return db.prepare(query)
    },
  }
  upstream.mockResolvedValueOnce(Response.json(deckLogAnswer({ sub_list: [] })))

  expect((await client(environment({ DECKLOG_DB: racing }))("jp/AB12C")).status).toBe(200)
  expect(((await stored()) as Record<string, unknown>)["cards"]).toBe(storedCards)
})
