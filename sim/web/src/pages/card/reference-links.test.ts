import { expect, it } from "vitest"

import { referenceLabel } from "./reference-links"

it.each([
  ["https://x.com/bo_tcg_event/status/1855883163387695327", "X @bo_tcg_event"],
  ["https://x.com/ShadowverseRAGE/status/1", "X @ShadowverseRAGE"],
  ["https://www.youtube.com/watch?v=abc123", "YouTube"],
  ["https://youtu.be/abc123", "YouTube"],
  ["https://shadowverse-evolve.com/news/post-234/", "shadowverse-evolve.com"],
  ["https://en.shadowverse-evolve.com/news/post-83/", "en.shadowverse-evolve.com"],
  ["https://sve-gp.com/archive/2024/nagoya/", "sve-gp.com"],
  ["https://en.bushiroad.com/bcs2526/prizes/", "en.bushiroad.com"],
])("labels %s as %s", (url, label) => {
  expect(referenceLabel(url)).toBe(label)
})

it("falls back to the text itself when it is not a URL", () => {
  expect(referenceLabel("not a url")).toBe("not a url")
})
