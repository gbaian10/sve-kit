// @vitest-environment node
import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"
import path from "node:path"

import { beforeAll, describe, expect, it } from "vitest"

import { initSync, PreparedMatch } from "./generated/engine"

beforeAll(() => {
  initSync({ module: readFileSync(path.join(import.meta.dirname, "generated/engine_bg.wasm")) })
})

const sha256 = (text: string) => createHash("sha256").update(text).digest("hex")
const content = (id: string, sha: string) => ({ id, sha256: sha })
const programs = JSON.stringify({ format_version: "4.2.0", entries: [], printings: {} })
const manifest = "1".repeat(64)

function inputs(programHash = sha256(programs)): string {
  return JSON.stringify({
    snapshot: "",
    keywords: JSON.stringify({ format: 1, kind: "keyword_registry", rule_names: {} }),
    rules: "{}",
    references: {
      region: "jp",
      snapshot_sha256: sha256(""),
      faces: [],
      wording: { manifest_sha256: `sha256:${manifest}`, faces: [] },
      rules: null,
    },
    printings: [],
    engine_identity: { Resolved: { region: "jp", snapshot_sha256: sha256(""), faces: [] } },
    identity: {
      snapshot: content("snapshot", manifest),
      programs: content("programs", programHash),
      engine: content("engine", "2".repeat(64)),
      rules: content("rules", sha256("{}")),
      region: "jp",
    },
  })
}

describe("engine WebAssembly", () => {
  it("rejects malformed inputs, an unpinned attachment and unusable programs with codes only", () => {
    expect(() => new PreparedMatch("{}", programs)).toThrow(/^format$/)
    expect(() => new PreparedMatch(inputs(), programs.replace("4.2.0", "4.1.0"))).toThrow(
      /^identity$/,
    )
    expect(() => new PreparedMatch(inputs(sha256("[")), "[")).toThrow(/^selection$/)
    expect(() => new PreparedMatch(inputs(), programs)).toThrow(/^selection$/)
  })
})
