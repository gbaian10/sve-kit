import { describe, expect, it } from "vitest"

import { copyLimit } from "./copyLimit"

describe("copyLimit", () => {
  it("is undefined without restrictions", () => {
    expect(copyLimit(undefined)).toBeUndefined()
    expect(copyLimit([])).toBeUndefined()
  })

  it("is undefined when no restriction sets a copy limit", () => {
    expect(copyLimit([{ maxCopies: null }, { maxCopies: null }])).toBeUndefined()
  })

  it("is the strictest limit, skipping restrictions without one", () => {
    expect(copyLimit([{ maxCopies: 2 }, { maxCopies: null }, { maxCopies: 1 }])).toBe(1)
  })

  it("keeps a ban as zero copies rather than no limit", () => {
    expect(copyLimit([{ maxCopies: 0 }, { maxCopies: 3 }])).toBe(0)
  })
})
