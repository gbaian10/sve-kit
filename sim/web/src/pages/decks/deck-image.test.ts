import { afterEach, describe, expect, it, vi } from "vitest"

import {
  canvasEncoder,
  DeckImageMissing,
  type Encode,
  encodeDeckImage,
  renderDeckImage,
} from "./deck-image"

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
    const colors = { page: "", text: "", muted: "", badge: "", badgeText: "" }
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

const colors = {
  page: "#fff",
  text: "#111",
  muted: "#666",
  badge: "#c00",
  badgeText: "#fff",
}

/** A 2D context that records what is drawn; jsdom has no canvas of its own. */
function recordingContext() {
  const calls: { name: string; args: unknown[] }[] = []
  const record =
    (name: string) =>
    (...args: unknown[]) => {
      calls.push({ name, args })
    }
  const ctx = new Proxy<Record<string, unknown>>(
    {},
    {
      get: (target, key) =>
        typeof key === "string" && key in target ? target[key] : record(String(key)),
    },
  )
  return {
    ctx,
    calls,
    texts: () => calls.filter((c) => c.name === "fillText").map((c) => c.args[0]),
  }
}

function card(label: string, count: number) {
  return { src: `/img/${label}`, label, count }
}

describe("deck image drawing", () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it("draws the leader, its line, every card with its count and skips an empty section", async () => {
    const { ctx, calls, texts } = recordingContext()
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
      ctx as unknown as CanvasRenderingContext2D,
    )
    const image = new Image()
    const canvas = await renderDeckImage(
      {
        size: "small",
        title: "Royal",
        subtitle: "JP",
        leader: { src: "/img/leader", label: "Leader" },
        leaderLine: "Leader: King",
        sections: [
          { label: "Main", cards: Array.from({ length: 11 }, (_, i) => card(`m${String(i)}`, 3)) },
          { label: "Evolve", cards: [] },
        ],
        footer: "2026-11-03",
        colors,
      },
      () => Promise.resolve(image),
    )
    // Eleven cards take two rows of ten: header 187 + section 356 + footer 40, rounded.
    expect(canvas.width).toBe(1200)
    expect(canvas.height).toBe(584)
    expect(calls.filter((c) => c.name === "drawImage")).toHaveLength(12)
    expect(texts()).toEqual([
      "Royal",
      "JP",
      "Leader: King",
      "Main",
      ...Array.from({ length: 11 }, () => "3"),
      "2026-11-03",
    ])
  })

  it("scales HD so each card is as wide as the largest card image, and omits the leader line", async () => {
    const { ctx, calls, texts } = recordingContext()
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
      ctx as unknown as CanvasRenderingContext2D,
    )
    const canvas = await renderDeckImage(
      {
        size: "hd",
        title: "No leader",
        subtitle: "",
        leader: null,
        leaderLine: "unused",
        sections: [{ label: "Main", cards: [card("a", 1)] }],
        footer: "",
        colors,
      },
      () => Promise.resolve(new Image()),
    )
    expect(canvas.width).toBe(5226)
    expect(calls.find((c) => c.name === "scale")?.args[0]).toBeCloseTo(5226 / 1200, 2)
    expect(texts()).not.toContain("unused")
  })

  it("fails when the browser gives no 2D context", async () => {
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null)
    await expect(
      renderDeckImage(
        {
          size: "small",
          title: "",
          subtitle: "",
          leader: null,
          leaderLine: "",
          sections: [],
          footer: "",
          colors,
        },
        () => Promise.resolve(null),
      ),
    ).rejects.toThrow("canvas unavailable")
  })

  it("loads card images with CORS and treats a failed download as missing", async () => {
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
      recordingContext().ctx as unknown as CanvasRenderingContext2D,
    )
    const loads: HTMLImageElement[] = []
    // jsdom never fetches images; the test settles each one when its src is set.
    vi.spyOn(HTMLImageElement.prototype, "src", "set").mockImplementation(function (
      this: HTMLImageElement,
      value: string,
    ) {
      loads.push(this)
      queueMicrotask(() => {
        if (value.endsWith("bad")) this.onerror?.(new Event("error"))
        else this.onload?.(new Event("load"))
      })
    })
    const input = (src: string) => ({
      size: "small" as const,
      title: "",
      subtitle: "",
      leader: null,
      leaderLine: "",
      sections: [{ label: "Main", cards: [{ src, label: src, count: 1 }] }],
      footer: "",
      colors,
    })
    await expect(renderDeckImage(input("/img/good"))).resolves.toBeInstanceOf(HTMLCanvasElement)
    await expect(renderDeckImage(input("/img/bad"))).rejects.toEqual(
      new DeckImageMissing(["/img/bad"]),
    )
    expect(loads.map((image) => image.crossOrigin)).toEqual(["anonymous", "anonymous"])
  })
})

describe("canvas encoder", () => {
  it("resolves to the browser's blob and fails when the browser gives none", async () => {
    const canvas = document.createElement("canvas")
    const blob = new Blob(["x"], { type: "image/webp" })
    const toBlob = vi
      .spyOn(canvas, "toBlob")
      .mockImplementationOnce((callback) => {
        callback(blob)
      })
      .mockImplementationOnce((callback) => {
        callback(null)
      })
    const encode = canvasEncoder(canvas)
    await expect(encode("image/webp", 0.9)).resolves.toBe(blob)
    expect(toBlob).toHaveBeenLastCalledWith(expect.any(Function), "image/webp", 0.9)
    await expect(encode("image/png")).rejects.toThrow("could not be encoded")
  })
})
