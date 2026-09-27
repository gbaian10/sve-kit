// @vitest-environment node
import { readdirSync, readFileSync } from "node:fs"
import path from "node:path"

import { ESLint } from "eslint"
import { beforeAll, describe, expect, it } from "vitest"

// Case syntax is described in README.md; every message counts, so duplicates show up.
const root = path.resolve(import.meta.dirname, "..")
const fixtureRoot = path.join(import.meta.dirname, "lint-fixtures")
const fixtureFiles = readdirSync(path.join(fixtureRoot, "src"), {
  recursive: true,
  encoding: "utf8",
})
  .filter((file) => /\.tsx?$/.test(file))
  .map((file) => path.join("src", file))
  .sort()

interface FixtureCase {
  file: string
  line: number
  title: string
  expected: string[]
  actual: string[]
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
      actual: [],
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
      owner.actual.push(message.ruleId ?? `fatal: ${message.message}`)
    }
  }
}, 60_000)

describe("eslint rule fixtures", () => {
  it("found the fixture cases", () => {
    expect(fixtureFiles.length).toBeGreaterThan(10)
    expect(cases.length).toBeGreaterThan(60)
  })

  it.each(cases.map((c) => [`${c.file}:${String(c.line)} ${c.title}`, c] as const))(
    "%s",
    (_name, c) => {
      expect([...c.actual].sort()).toEqual([...c.expected].sort())
    },
  )
})
