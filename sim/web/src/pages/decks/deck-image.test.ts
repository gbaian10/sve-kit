import { describe, expect, it } from "vitest"

import { DeckImageMissing, type Encode, encodeDeckImage, renderDeckImage } from "./deck-image"

// The issue's target: an HD picture small enough for a free Discord upload.
const HD_BUDGET_BYTES = 3 * 1024 * 1024

/** A fake encoder: sizes fall with quality; `webp` false behaves like a browser without WebP. */
function encoder(bytesAt: (quality: number) => number, webp = true) {
  const calls: [string, number | undefined][] = []
  const encode: Encode = (type, quality) => {
    calls.push([type, quality])
    const actual = type === "image/webp" && !webp ? "image/png" : type
    return Promise.resolve(new Blob([new Uint8Array(bytesAt(quality ?? 1))], { type: actual }))
  }
  return { encode, calls }
}

describe("deck image encoding", () => {
  it("keeps the small picture as PNG", async () => {
    const { encode, calls } = encoder(() => 800_000)
    expect((await encodeDeckImage("small", encode)).type).toBe("image/png")
    expect(calls).toEqual([["image/png", undefined]])
  })

  it("encodes HD as WebP at the best quality under the budget", async () => {
    const { encode } = encoder((quality) => (quality > 0.8 ? HD_BUDGET_BYTES + 1 : 2_500_000))
    const blob = await encodeDeckImage("hd", encode)
    expect(blob.type).toBe("image/webp")
    expect(blob.size).toBeLessThanOrEqual(HD_BUDGET_BYTES)
  })

  it("falls back to JPEG where the browser cannot encode WebP", async () => {
    const { encode } = encoder(() => 2_000_000, false)
    expect((await encodeDeckImage("hd", encode)).type).toBe("image/jpeg")
  })

  it("stops at the lowest quality when nothing fits", async () => {
    const { encode, calls } = encoder(() => HD_BUDGET_BYTES * 2)
    const blob = await encodeDeckImage("hd", encode)
    expect(blob.size).toBeGreaterThan(HD_BUDGET_BYTES)
    expect(calls.at(-1)).toEqual(["image/webp", 0.5])
  })

  it("refuses to draw when any card image is missing or fails to load", async () => {
    const colors = { page: "", text: "", muted: "", badge: "", badgeText: "", empty: "" }
    const load = (src: string) => Promise.resolve(src === "ok" ? new Image() : null)
    const drawing = renderDeckImage(
      {
        size: "small",
        title: "D",
        subtitle: "",
        leader: { src: "ok", label: "Leader" },
        leaderLine: "",
        sections: [
          {
            label: "Main",
            cards: [
              { src: "ok", label: "Loaded", count: 3 },
              { src: "404", label: "Broken", count: 2 },
              { src: undefined, label: "No data", count: 1 },
            ],
          },
        ],
        footer: "",
        colors,
      },
      load,
    )
    await expect(drawing).rejects.toEqual(new DeckImageMissing(["Broken", "No data"]))
    await expect(drawing).rejects.toMatchObject({ cards: ["Broken", "No data"] })
  })
})
