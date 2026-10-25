import { describe, expect, it } from "vitest"
import type { TestModule } from "vitest/node"

import { publicFile, publicJunit, publicName } from "./public-junit-reporter"

const PRIVATE = "合成卡文_PRIVATE_SENTINEL"

interface FakeCase {
  fullName: string
  state: "failed" | "passed" | "pending" | "skipped"
  retryCount?: number
  duration?: number
}

function module(relativeModuleId: string, cases: FakeCase[]): TestModule {
  return {
    relativeModuleId,
    children: {
      *allTests() {
        for (const test of cases) {
          yield {
            fullName: test.fullName,
            result: () => ({ state: test.state }),
            diagnostic: () => ({ retryCount: test.retryCount ?? 0, duration: test.duration ?? 0 }),
          }
        }
      },
    },
  } as unknown as TestModule
}

describe("public JUnit for Codecov Test Analytics", () => {
  it("never carries titles, paths outside the allow list or messages", () => {
    const xml = publicJunit([
      module(`src/pages/${PRIVATE}.test.tsx`, [{ fullName: `suite ${PRIVATE}`, state: "passed" }]),
      module("src/app/ok.test.ts", [{ fullName: `suite ${PRIVATE}`, state: "failed" }]),
    ])
    expect(xml).not.toContain(PRIVATE)
    expect(xml).toContain('classname="unknown"')
    expect(xml).toContain('classname="src/app/ok.test.ts"')
  })

  it("records a retried test that finally passed as failures followed by a pass", () => {
    const xml = publicJunit([
      module("src/app/a.test.ts", [
        { fullName: "flaky", state: "passed", retryCount: 2, duration: 1500 },
        { fullName: "steady", state: "passed", duration: 10 },
        { fullName: "broken", state: "failed", retryCount: 1, duration: 20 },
        { fullName: "later", state: "skipped" },
        { fullName: "todo", state: "pending" },
      ]),
    ])
    const flaky = publicName("flaky")
    expect(xml.match(new RegExp(`name="${flaky}"`, "g"))).toHaveLength(3)
    expect(xml.match(new RegExp(`name="${flaky}"[^>]*><failure`, "g"))).toHaveLength(2)
    expect(xml).toContain('tests="8" failures="4" errors="0" skipped="2"')
    expect(xml).toContain(`name="${publicName("steady")}" time="0.010000"/>`)
  })

  it("keeps a case's name stable and distinct per title", () => {
    expect(publicName("a")).toBe(publicName("a"))
    expect(publicName("a")).not.toBe(publicName("b"))
    expect(publicName("a")).toMatch(/^case_[0-9a-f]{8}$/)
  })

  it("only accepts repository test paths", () => {
    expect(publicFile("src/x/y.test.tsx")).toBe("src/x/y.test.tsx")
    expect(publicFile("../src/x.test.ts")).toBe("unknown")
    expect(publicFile("/abs/src/x.test.ts")).toBe("unknown")
    expect(publicFile("src/x/ü.test.ts")).toBe("unknown")
  })
})
