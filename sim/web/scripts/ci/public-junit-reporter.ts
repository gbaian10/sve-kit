import { createHash } from "node:crypto"
import { writeFileSync } from "node:fs"

import type { Reporter, TestCase, TestModule } from "vitest/node"

// Test titles are free text and may quote card wording, so a case is named by a hash of its title and
// located by its file; failure messages and output never leave the runner.
const SAFE_FILE = /^(?:src|tests|scripts)\/[A-Za-z0-9_./-]+\.(?:ts|tsx)$/

export function publicName(fullName: string): string {
  return `case_${createHash("sha256").update(fullName).digest("hex").slice(0, 8)}`
}

export function publicFile(relativeModuleId: string): string {
  return SAFE_FILE.test(relativeModuleId) && !relativeModuleId.includes("..")
    ? relativeModuleId
    : "unknown"
}

function seconds(milliseconds: number): string {
  return (Math.max(milliseconds, 0) / 1000).toFixed(6)
}

interface Counts {
  tests: number
  failures: number
  skipped: number
  time: number
}

function entries(module: TestModule, counts: Counts): string[] {
  const classname = publicFile(module.relativeModuleId)
  const lines: string[] = []
  for (const test of module.children.allTests()) {
    const name = publicName(test.fullName)
    const state = test.result().state
    const { retryCount = 0, duration = 0 } = test.diagnostic() ?? {}
    const attempt = (outcome: "failed" | "passed" | "skipped", time: number): void => {
      counts.tests += 1
      counts.time += time
      if (outcome === "failed") counts.failures += 1
      if (outcome === "skipped") counts.skipped += 1
      const body =
        outcome === "failed"
          ? `<failure message="failed"/>`
          : outcome === "skipped"
            ? `<skipped/>`
            : ""
      const open = `<testcase classname="${classname}" name="${name}" time="${seconds(time)}"`
      lines.push(body === "" ? `${open}/>` : `${open}>${body}</testcase>`)
    }
    if (state === "pending" || state === "skipped") {
      attempt("skipped", 0)
      continue
    }
    // Earlier attempts failed; a retried test that finally passed is flaky, exactly what Codecov looks for.
    for (let index = 0; index < retryCount; index += 1) attempt("failed", 0)
    attempt(state === "passed" ? "passed" : "failed", duration)
  }
  return lines
}

export function publicJunit(modules: readonly TestModule[]): string {
  const counts: Counts = { tests: 0, failures: 0, skipped: 0, time: 0 }
  const lines = modules.flatMap((module) => entries(module, counts))
  const total = String(counts.tests)
  const failed = String(counts.failures)
  const skipped = String(counts.skipped)
  const suite =
    `<testsuite name="vitest" tests="${total}" failures="${failed}" errors="0"` +
    ` skipped="${skipped}" time="${seconds(counts.time)}">`
  return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuites>\n${suite}\n${lines.join("\n")}\n</testsuite>\n</testsuites>\n`
}

// Writes only when CI names an output file, so local runs and the default reporters are unaffected.
export default class PublicJunitReporter implements Reporter {
  onTestRunEnd(testModules: readonly TestModule[]): void {
    const output = process.env["SVE_PUBLIC_JUNIT"]
    if (output === undefined || output === "") return
    writeFileSync(output, publicJunit(testModules), { mode: 0o600 })
  }
}

export type { TestCase }
