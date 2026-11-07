import { orderCorners, type Point, type Quad } from "./tensor"

export interface Pixels {
  readonly width: number
  readonly height: number
  /** RGBA, row major. */
  readonly data: Uint8ClampedArray
}

/** The 3×3 projective map (row major, last entry 1) taking each `src` corner to its `dst` corner. */
function homography(src: Quad, dst: Quad): number[] {
  const rows: number[][] = []
  for (let i = 0; i < 4; i += 1) {
    const [x, y] = src[i] as Point
    const [u, v] = dst[i] as Point
    rows.push([x, y, 1, 0, 0, 0, -u * x, -u * y, u])
    rows.push([0, 0, 0, x, y, 1, -v * x, -v * y, v])
  }
  // Gauss–Jordan elimination with partial pivoting on the 8×9 augmented system.
  for (let c = 0; c < 8; c += 1) {
    let pivot = c
    for (let r = c + 1; r < 8; r += 1)
      if (Math.abs(rows[r]?.[c] ?? 0) > Math.abs(rows[pivot]?.[c] ?? 0)) pivot = r
    const swap = rows[c] as number[]
    rows[c] = rows[pivot] as number[]
    rows[pivot] = swap
    const top = rows[c] as number[]
    for (let r = 0; r < 8; r += 1) {
      if (r === c) continue
      const row = rows[r] as number[]
      const f = (row[c] ?? 0) / (top[c] ?? 1)
      for (let j = c; j < 9; j += 1) row[j] = (row[j] ?? 0) - f * (top[j] ?? 0)
    }
  }
  return [...rows.map((row, i) => (row[8] ?? 0) / (row[i] ?? 1)), 1]
}

/**
 * Corners as seen: top left, top right, bottom right, bottom left. With `sideways` the card lies on
 * its side, so its own top edge is the left edge on screen.
 */
export function orderQuad(quad: readonly Point[], sideways = false): Quad {
  const o = orderCorners(quad)
  return sideways ? [o[3], o[0], o[1], o[2]] : o
}

/** Straightens the card at `quad` in `src` into a `width` × `height` image (bilinear). */
export function warpQuad(
  src: Pixels,
  quad: Quad,
  width: number,
  height: number,
): Uint8ClampedArray {
  const m = homography(
    [
      [0, 0],
      [width, 0],
      [width, height],
      [0, height],
    ],
    quad,
  )
  const [m0, m1, m2, m3, m4, m5, m6, m7] = m as [
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
  ]
  const out = new Uint8ClampedArray(width * height * 4)
  const sw = src.width
  const sd = src.data
  const maxU = src.width - 1.001
  const maxV = src.height - 1.001
  for (let y = 0; y < height; y += 1)
    for (let x = 0; x < width; x += 1) {
      const X = x + 0.5
      const Y = y + 0.5
      const w = m6 * X + m7 * Y + 1
      const u = Math.min(Math.max((m0 * X + m1 * Y + m2) / w - 0.5, 0), maxU)
      const v = Math.min(Math.max((m3 * X + m4 * Y + m5) / w - 0.5, 0), maxV)
      const iu = Math.floor(u)
      const iv = Math.floor(v)
      const fu = u - iu
      const fv = v - iv
      const i00 = (iv * sw + iu) * 4
      const i01 = i00 + sw * 4
      const o = (y * width + x) * 4
      for (let ch = 0; ch < 3; ch += 1) {
        const a = sd[i00 + ch] ?? 0
        const b = sd[i00 + 4 + ch] ?? 0
        const c = sd[i01 + ch] ?? 0
        const d = sd[i01 + 4 + ch] ?? 0
        const top = a + (b - a) * fu
        const bottom = c + (d - c) * fu
        out[o + ch] = top + (bottom - top) * fv
      }
      out[o + 3] = 255
    }
  return out
}
