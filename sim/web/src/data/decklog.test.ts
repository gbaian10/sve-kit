import { afterEach, expect, it, vi } from "vitest"

import { fetchDeckLogDeck } from "./decklog"

afterEach(() => {
  vi.unstubAllGlobals()
})

const answer = {
  region: "jp",
  code: "AB12C",
  format: "N",
  className: null,
  leader: [],
  main: [{ cardNo: "BP01-001", count: 3 }],
  evolve: [],
}

function respond(response: Response | Promise<Response>) {
  const stub = vi.fn<typeof fetch>().mockReturnValue(Promise.resolve(response))
  vi.stubGlobal("fetch", stub)
  return stub
}

it("asks the Worker and accepts only a complete deck for the code asked", async () => {
  const stub = respond(Response.json(answer))
  expect(await fetchDeckLogDeck("jp", "AB12C")).toEqual({ deck: answer })
  expect(stub.mock.calls[0]?.[0]).toBe("/api/decklog/jp/AB12C")

  for (const body of [
    { ...answer, main: [] },
    { ...answer, evolve: null },
    { ...answer, code: "OTHER" },
  ]) {
    respond(Response.json(body))
    expect(await fetchDeckLogDeck("jp", "AB12C")).toEqual({ error: "upstream-invalid" })
  }
})

it("passes the Worker's error codes on and names the rest", async () => {
  respond(Response.json({ error: "not-sve" }, { status: 422 }))
  expect(await fetchDeckLogDeck("jp", "AB12C")).toEqual({ error: "not-sve" })
  respond(Response.json({ error: "something-new" }, { status: 500 }))
  expect(await fetchDeckLogDeck("jp", "AB12C")).toEqual({ error: "server-error" })
  respond(new Response("<html>", { status: 502 }))
  expect(await fetchDeckLogDeck("jp", "AB12C")).toEqual({ error: "server-error" })
  vi.stubGlobal("fetch", () => Promise.reject(new TypeError("offline")))
  expect(await fetchDeckLogDeck("jp", "AB12C")).toEqual({ error: "offline" })
})

it("rejects instead of answering once the caller has given up", async () => {
  const controller = new AbortController()
  respond(
    new Response(
      new ReadableStream({
        start(stream) {
          controller.abort()
          stream.error(new DOMException("Aborted", "AbortError"))
        },
      }),
    ),
  )
  await expect(fetchDeckLogDeck("jp", "AB12C", controller.signal)).rejects.toThrow()
})
