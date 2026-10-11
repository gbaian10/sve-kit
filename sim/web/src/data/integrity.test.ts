// @vitest-environment node
import { createHash } from "node:crypto"

import { expect, it } from "vitest"

import { transferDigest } from "./integrity"

it("hashes only the selected bytes with native SHA-256", async () => {
  const bytes = new TextEncoder().encode("ignored-prefix:published-json:ignored-suffix")
  const view = bytes.subarray(15, 29)
  expect(await transferDigest(view)).toBe(
    `sha256:${createHash("sha256").update(view).digest("hex")}`,
  )
  expect(await transferDigest(new Uint8Array())).toBe(
    "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  )
})
