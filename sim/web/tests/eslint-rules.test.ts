// @vitest-environment node
import { readFileSync } from "node:fs"
import path from "node:path"

import { ESLint } from "eslint"
import { beforeAll, describe, expect, it } from "vitest"

/*
 * Every fixture case is a `// case: <what> -> <rule ids | none>` (or `// bypass:`) comment
 * followed by code. Each lint message is attributed to the nearest case above it, and the
 * set of rules reported for a case must equal the set it declares. Bypass cases document
 * holes the rules cannot close, so they must stay silent until a rule starts catching them.
 */

const root = path.resolve(import.meta.dirname, "..")
const fixtureRoot = path.join(import.meta.dirname, "lint-fixtures")
const fixtureFiles = [
  "src/components/theme.tsx",
  "src/components/rwd.tsx",
  "src/components/text.tsx",
  "src/components/logic.ts",
  "src/components/boundaries.ts",
  "src/components/markup.tsx",
  "src/data/index.ts",
  "src/data/cache.ts",
  "src/settings/index.ts",
]

interface FixtureCase {
  file: string
  line: number
  title: string
  expected: string[]
  actual: Set<string>
}

const CASE = /^\s*\/\/ (case|bypass): (.+) -> (.+)$/

function parseCases(file: string): FixtureCase[] {
  const lines = readFileSync(path.join(fixtureRoot, file), "utf8").split("\n")
  return lines.flatMap((text, index) => {
    const match = CASE.exec(text)
    if (!match) return []
    const [, kind = "", title = "", rules = ""] = match
    return {
      file,
      line: index + 1,
      title: `${kind}: ${title}`,
      expected: rules.trim() === "none" ? [] : rules.split(",").map((rule) => rule.trim()),
      actual: new Set<string>(),
    }
  })
}

const cases = fixtureFiles.flatMap(parseCases)

beforeAll(async () => {
  const eslint = new ESLint({
    cwd: fixtureRoot,
    overrideConfigFile: path.join(root, "eslint.config.ts"),
    ignore: false,
  })
  const results = await eslint.lintFiles(fixtureFiles)
  for (const result of results) {
    const file = path.relative(fixtureRoot, result.filePath)
    const fileCases = cases.filter((c) => c.file === file)
    for (const message of result.messages) {
      const owner = fileCases.findLast((c) => c.line < message.line)
      if (!owner) throw new Error(`${file}:${String(message.line)} is outside any case`)
      owner.actual.add(message.ruleId ?? `fatal: ${message.message}`)
    }
  }
}, 60_000)

describe("eslint rule fixtures", () => {
  it("found the fixture cases", () => {
    expect(cases.length).toBeGreaterThan(40)
  })

  it.each(cases.map((c) => [`${c.file}:${String(c.line)} ${c.title}`, c] as const))(
    "%s",
    (_name, c) => {
      expect([...c.actual].sort()).toEqual([...c.expected].sort())
    },
  )
})
