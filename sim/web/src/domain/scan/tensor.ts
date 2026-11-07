export type Point = readonly [number, number]
/** Card corners in image pixels: top left, top right, bottom right, bottom left as seen. */
export type Quad = readonly [Point, Point, Point, Point]

export interface Detection {
  /** Card-centre confidence, 0–1. */
  readonly score: number
  readonly quad: Quad
}

/**
 * The finder gives corners clockwise from any corner (format 1): sort them by angle around their
 * mean, then start at the one with the smallest x + y. The result is top left, top right, bottom
 * right, bottom left on screen.
 */
export function orderCorners(quad: readonly Point[]): Quad {
  const cx = quad.reduce((sum, p) => sum + p[0], 0) / quad.length
  const cy = quad.reduce((sum, p) => sum + p[1], 0) / quad.length
  const byAngle = [...quad].sort(
    (a, b) => Math.atan2(a[1] - cy, a[0] - cx) - Math.atan2(b[1] - cy, b[0] - cx),
  )
  let first = 0
  for (let i = 1; i < byAngle.length; i += 1) {
    const p = byAngle[i] as Point
    const q = byAngle[first] as Point
    if (p[0] + p[1] < q[0] + q[1]) first = i
  }
  return [...byAngle.slice(first), ...byAngle.slice(0, first)] as unknown as Quad
}

/**
 * Card finder output to ordered corner quads in input pixels. `heat` is one G×G map of card
 * centres; `corners` holds, per cell, the four corners' x/y offsets from the cell centre in cells.
 * Only local maxima at or above `threshold` count, strongest first. `grow` scales each quad about
 * its mean point, as the finder's calibration says.
 */
export function decodeDetections(
  heat: Float32Array,
  corners: Float32Array,
  grid: number,
  stride: number,
  threshold: number,
  grow = 1,
): Detection[] {
  const cells = grid * grid
  const out: Detection[] = []
  for (let i = 0; i < cells; i += 1) {
    const p = heat[i] ?? 0
    if (p < threshold) continue
    const gy = Math.floor(i / grid)
    const gx = i % grid
    let peak = true
    for (let dy = -1; dy <= 1 && peak; dy += 1)
      for (let dx = -1; dx <= 1; dx += 1) {
        const y = gy + dy
        const x = gx + dx
        if (
          (dx || dy) &&
          y >= 0 &&
          y < grid &&
          x >= 0 &&
          x < grid &&
          (heat[y * grid + x] ?? 0) > p
        ) {
          peak = false
          break
        }
      }
    if (!peak) continue
    const corner = (c: number): Point => [
      (gx + 0.5 + (corners[2 * c * cells + i] ?? 0)) * stride,
      (gy + 0.5 + (corners[(2 * c + 1) * cells + i] ?? 0)) * stride,
    ]
    const raw = [corner(0), corner(1), corner(2), corner(3)]
    const mx = raw.reduce((sum, q) => sum + q[0], 0) / 4
    const my = raw.reduce((sum, q) => sum + q[1], 0) / 4
    const grown = raw.map(([x, y]): Point => [mx + (x - mx) * grow, my + (y - my) * grow])
    out.push({ score: p, quad: orderCorners(grown) })
  }
  return out.sort((a, b) => b.score - a.score)
}

/**
 * Writes RGBA pixels as one float32 sample of an NCHW batch (RGB planes, 0–1) at sample `slot`.
 * `turned` writes the image rotated 180°, so an upside-down card reads the same.
 */
export function writeSample(
  rgba: Uint8ClampedArray,
  batch: Float32Array,
  slot: number,
  turned = false,
): void {
  const plane = rgba.length / 4
  const base = slot * 3 * plane
  for (let i = 0; i < plane; i += 1) {
    const j = turned ? plane - 1 - i : i
    batch[base + j] = (rgba[4 * i] ?? 0) / 255
    batch[base + plane + j] = (rgba[4 * i + 1] ?? 0) / 255
    batch[base + 2 * plane + j] = (rgba[4 * i + 2] ?? 0) / 255
  }
}
