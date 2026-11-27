// @vitest-environment node
import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"
import path from "node:path"

import { beforeAll, describe, expect, it } from "vitest"

import { initSync, PreparedMatch } from "./generated/engine"
import { BattleHost } from "./host"
import type { MatchOptions, SeatMessage } from "./protocol"

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

function receive(port: MessagePort): Promise<SeatMessage> {
  return new Promise((resolve) => {
    port.onmessage = (event: MessageEvent<SeatMessage>) => {
      resolve(event.data)
    }
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

  it("routes two different WebAssembly deck errors to the submitting port", async () => {
    const [document, attachment] = JSON.parse(
      readFileSync(
        path.join(
          import.meta.dirname,
          "../../../../battle-host/tests/fixtures/published-battle.json",
        ),
        "utf8",
      ),
    ) as [unknown, string]
    const prepared = new PreparedMatch(JSON.stringify(document), attachment)
    const deck = (printing: string): MatchOptions["decks"][number] => ({
      construction: { kind: "class", value: "ニュートラル" },
      main: Array.from({ length: 8 }, () => printing),
      evolve: [],
    })
    try {
      for (const [printing, code] of [
        ["unavailable", "deck_printing"],
        ["test-t-unused", "deck_admission"],
      ] as const) {
        const p1 = new MessageChannel()
        const p2 = new MessageChannel()
        try {
          const received = Promise.all([receive(p1.port1), receive(p2.port1)])
          const host = new BattleHost((setup) => Promise.resolve(prepared.open(setup)))
          host.init({
            kind: "init",
            options: {
              decks: [deck("test-t-anchor"), deck(printing)],
              control: ["player", "player"],
              first_chooser: "p1",
              seed: "load",
              max_submissions: 10,
            },
            ports: { p1: p1.port2, p2: p2.port2 },
          })
          await expect(received).resolves.toEqual([
            { kind: "load_error", code: "match" },
            { kind: "load_error", code },
          ])
        } finally {
          for (const channel of [p1, p2]) {
            channel.port1.close()
            channel.port2.close()
          }
        }
      }
    } finally {
      prepared.free()
    }
  })
})
