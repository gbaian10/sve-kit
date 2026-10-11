import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import { expect, it } from "vitest"

import { buildIdentity } from "../vite.config"

it("changes with shipped sources and locked inputs, not with tests or missing files", () => {
  const root = mkdtempSync(path.join(tmpdir(), "build-id-"))
  const src = path.join(root, "src")
  const lock = path.join(root, "bun.lock")
  mkdirSync(path.join(src, "test-utils"), { recursive: true })
  writeFileSync(path.join(src, "engine.ts"), "export const a = 1\n")
  writeFileSync(lock, "lock 1\n")
  const inputs = [src, lock, path.join(root, "missing.js")]
  const initial = buildIdentity(inputs, root)
  expect(initial).toMatch(/^[0-9a-f]{16}$/)
  writeFileSync(path.join(src, "engine.test.ts"), "test\n")
  writeFileSync(path.join(src, "test-utils", "cache.ts"), "helper\n")
  writeFileSync(path.join(src, "test-setup.ts"), "setup\n")
  expect(buildIdentity(inputs, root)).toBe(initial)
  writeFileSync(path.join(src, "engine.ts"), "export const a = 2\n")
  const edited = buildIdentity(inputs, root)
  expect(edited).not.toBe(initial)
  writeFileSync(lock, "lock 2\n")
  expect(buildIdentity(inputs, root)).not.toBe(edited)
})
