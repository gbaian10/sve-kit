import { describe, expect, it } from "vitest"

import type { CardMatch, ScanCard } from "./search"
import { ScanTracker } from "./tracker"

const card = (id: string): ScanCard => ({
  no: `BP01-${id}`,
  cardId: id,
  imageId: 1,
  face: 0,
  name: id,
  artGroup: "a",
  classCode: "",
})
const A = card("001")
const B = card("002")
const C = card("003")
const match = (c: ScanCard, raw: number): CardMatch => ({ card: c, raw, score: raw })
const view = (...values: number[]) => Float32Array.from(values)
const sure = (picks: readonly CardMatch[]) => (picks[0]?.raw ?? 0) >= 0.5

describe("scan tracker", () => {
  it("averages each view over recent frames and starts over when the camera moves away", () => {
    const tracker = new ScanTracker(2)
    tracker.smooth([view(1, 0)])
    const [mean] = tracker.smooth([view(0.8, 0.6)])
    expect(mean?.[0]).toBeCloseTo(1.8 / Math.hypot(1.8, 0.6))
    const [fresh] = tracker.smooth([view(0, 1)])
    expect(Array.from(fresh ?? [])).toEqual([0, 1])
  })

  it("shows a confident card as working at first and as recognised once it has held", () => {
    const tracker = new ScanTracker(2)
    const scene = view(1, 0)
    expect(tracker.step(scene, [match(A, 0.9), match(B, 0.4)], 0, sure).status).toBe("busy")
    expect(tracker.step(scene, [match(A, 0.9), match(B, 0.4)], 300, sure).status).toBe("busy")
    const held = tracker.step(scene, [match(A, 0.9), match(B, 0.4)], 650, sure)
    expect(held.status).toBe("ok")
    expect(held.picks.map((pick) => pick.card)).toEqual([A, B])
    // One weak frame does not undo a held answer; the recogniser's own check decides.
    expect(tracker.step(scene, [match(A, 0.3)], 700, sure).status).toBe("ok")
    expect(tracker.step(scene, [match(A, 0.9)], 750, () => false).status).toBe("unsure")
  })

  it("lets a new card lead only after it stayed ahead for half a second", () => {
    const tracker = new ScanTracker(2)
    const scene = view(1, 0)
    for (let t = 0; t <= 300; t += 100) tracker.step(scene, [match(A, 0.9), match(B, 0.5)], t, sure)
    let frame = tracker.step(scene, [match(B, 0.99), match(A, 0.1)], 1500, sure)
    expect(frame.picks[0]?.card).toBe(A)
    frame = tracker.step(scene, [match(B, 0.99), match(A, 0.1)], 1800, sure)
    expect(frame.picks[0]?.card).toBe(A)
    frame = tracker.step(scene, [match(B, 0.99), match(A, 0.1)], 2100, sure)
    expect(frame.picks[0]?.card).toBe(B)
  })

  it("drops the old card at once when the view changes for two frames", () => {
    const tracker = new ScanTracker(2)
    for (let t = 0; t <= 300; t += 100) tracker.step(view(1, 0), [match(A, 0.9)], t, sure)
    tracker.step(view(0, 1), [match(C, 0.9)], 400, sure)
    const frame = tracker.step(view(0, 1), [match(C, 0.9)], 500, sure)
    expect(frame.picks.map((pick) => pick.card)).toEqual([C])
  })

  it("keeps recently seen cards for a few seconds, apart from the current picks", () => {
    const tracker = new ScanTracker(2)
    tracker.step(view(1, 0), [match(A, 0.9), match(B, 0.6)], 0, sure)
    tracker.reset()
    let frame = tracker.step(view(0, 1), [match(C, 0.9)], 1000, sure)
    expect(frame.recent).toEqual([A, B])
    frame = tracker.step(view(0, 1), [match(C, 0.9)], 9500, sure)
    expect(frame.recent).toEqual([])
    tracker.step(view(1, 0), [match(A, 0.9)], 9600, sure)
    tracker.clear()
    expect(tracker.step(view(0, 1), [match(C, 0.9)], 9700, sure).recent).toEqual([])
  })
})
