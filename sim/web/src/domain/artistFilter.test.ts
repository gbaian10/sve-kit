import { describe, expect, it } from "vitest"

import {
  CREDITED_ARTIST,
  creditedState,
  orderArtistTokens,
  toggleArtist,
  toggleCredited,
  UNCREDITED_ARTIST,
} from "./artistFilter"
import { formatQuery, parseQuery } from "./query/codec"
import { DEFAULT_QUERY } from "./query/model"

describe("artist filter tokens", () => {
  it("never collides with an artist id, which always reads y:<32 hex>", () => {
    for (const word of [UNCREDITED_ARTIST, CREDITED_ARTIST]) {
      expect(word).not.toMatch(/^y:[0-9a-f]{32}$/u)
      expect(word).not.toContain(":")
    }
    expect(UNCREDITED_ARTIST).not.toBe(CREDITED_ARTIST)
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

  it("round-trips the credited word through the URL and puts it first", () => {
    const id = `y:${"a".repeat(32)}`
    const state = parseQuery(new URLSearchParams(`artist=uncredited,${id},credited`))
    expect(orderArtistTokens(state.artists)).toEqual([CREDITED_ARTIST, id, UNCREDITED_ARTIST])
    expect(formatQuery({ ...DEFAULT_QUERY, artists: [CREDITED_ARTIST] }).toString()).toBe(
      "artist=credited",
    )
  })

  it("ticks everything at once, unticks one from all, and collapses a full list", () => {
    const all = ["y:a", "y:b", "y:c"]
    expect(creditedState([], all)).toBe("none")
    expect(creditedState(["y:b"], all)).toBe("some")
    expect(creditedState(all, all)).toBe("all")
    expect(toggleCredited([], all)).toEqual([CREDITED_ARTIST])
    expect(toggleCredited(["y:a", UNCREDITED_ARTIST], all)).toEqual([
      CREDITED_ARTIST,
      UNCREDITED_ARTIST,
    ])
    expect(toggleCredited([CREDITED_ARTIST, UNCREDITED_ARTIST], all)).toEqual([UNCREDITED_ARTIST])
    expect(toggleArtist([CREDITED_ARTIST], "y:b", all)).toEqual(["y:a", "y:c"])
    expect(toggleArtist(["y:a", "y:c"], "y:b", all)).toEqual([CREDITED_ARTIST])
    expect(toggleArtist(["y:a", UNCREDITED_ARTIST], "y:b", all)).toEqual([
      "y:a",
      UNCREDITED_ARTIST,
      "y:b",
    ])
  })
})
