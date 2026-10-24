import { describe, expect, it } from "vitest"

import { orderArtistTokens, UNCREDITED_ARTIST } from "./artistFilter"
import { formatQuery, parseQuery } from "./query/codec"
import { DEFAULT_QUERY } from "./query/model"

describe("artist filter tokens", () => {
  it("never collides with an artist id, which always reads y:<32 hex>", () => {
    expect(UNCREDITED_ARTIST).not.toMatch(/^y:[0-9a-f]{32}$/u)
    expect(UNCREDITED_ARTIST).not.toContain(":")
  })

  it("travels in the URL beside artist ids and sorts last", () => {
    const id = `y:${"a".repeat(32)}`
    const state = parseQuery(new URLSearchParams(`artist=${UNCREDITED_ARTIST},${id}`))
    expect(state.artists).toEqual([id, UNCREDITED_ARTIST].sort())
    expect(orderArtistTokens(state.artists)).toEqual([id, UNCREDITED_ARTIST])
    expect(formatQuery({ ...DEFAULT_QUERY, artists: [UNCREDITED_ARTIST] }).toString()).toBe(
      "artist=uncredited",
    )
  })
})
