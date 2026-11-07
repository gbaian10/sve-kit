import { afterEach, describe, expect, it, vi } from "vitest"

import { createIndexDots } from "./dots"
import { matchCards, parseScanCards, type ScanIndex } from "./search"

const DIM = 256
const ROWS = 600

/** A fixed pseudo-random sequence, so a failure can be reproduced. */
function random(seed: number): () => number {
  let s = seed
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648
    return s / 2147483648
  }
}

const next = random(7)
const vectors = Int8Array.from({ length: ROWS * DIM }, () => Math.round(next() * 254 - 127))

function unit(): Float32Array {
  const v = Float32Array.from({ length: DIM }, () => next() * 2 - 1)
  const n = Math.hypot(...v)
  return v.map((x) => x / n)
}

function plainBest(views: readonly Float32Array[]): number[] {
  return Array.from({ length: ROWS }, (_, i) => {
    let top = -Infinity
    for (const view of views) {
      let dot = 0
      for (let j = 0; j < DIM; j += 1) dot += (vectors[i * DIM + j] ?? 0) * (view[j] ?? 0)
      top = Math.max(top, dot)
    }
    return top
  })
}

afterEach(() => {
  vi.restoreAllMocks()
})

describe("SIMD card scores", () => {
  it("gives every row the same best dot product as plain JavaScript", () => {
    const dots = createIndexDots(vectors, DIM)
    expect(dots).not.toBeNull()
    for (const views of [[unit()], [unit(), unit()], [unit(), unit(), unit()]]) {
      const want = plainBest(views)
      const got = Array.from(dots?.best(views) ?? [])
      const worst = Math.max(...want.map((w, i) => Math.abs(w - (got[i] ?? 0))))
      // The query is rounded to 16 bits: the error stays far below the score gaps that matter.
      expect(worst).toBeLessThan(0.05)
    }
  })

  it("ranks cards the same as the plain JavaScript search", () => {
    const rows = Array.from({ length: ROWS }, (_, i) => ({
      no: `BP01-${String(i + 1).padStart(3, "0")}`,
      c: `c${String(i)}`,
      n: i + 1,
      f: 0,
      nm: "x",
      a: `a${String(i)}`,
      k: "",
    }))
    const plain: ScanIndex = {
      dim: DIM,
      vectors,
      scale: new Float32Array(ROWS).fill(1 / (127 * 16)),
      hub: Float32Array.from({ length: ROWS }, () => next() * 0.1),
      cards: parseScanCards(rows, ROWS),
      scoring: { hubWeight: 0.75, accept: 0.5, margin: 0.03 },
    }
    const fast = { ...plain, dots: createIndexDots(vectors, DIM) }
    const views = [unit(), unit()]
    const a = matchCards(plain, views, null, 10)
    const b = matchCards(fast, views, null, 10)
    expect(b.map((m) => m.card.no)).toEqual(a.map((m) => m.card.no))
    b.forEach((m, k) => {
      expect(m.raw).toBeCloseTo(a[k]?.raw ?? Number.NaN, 4)
    })
  })

  it("leaves scoring to JavaScript when the index is not 256 wide or the browser lacks SIMD", () => {
    expect(createIndexDots(new Int8Array(128 * 4), 128)).toBeNull()
    vi.spyOn(WebAssembly, "validate").mockReturnValue(false)
    expect(createIndexDots(vectors, DIM)).toBeNull()
  })
})
