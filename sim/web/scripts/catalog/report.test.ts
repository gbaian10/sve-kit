import { describe, expect, it } from "vitest"

import { type Report, type Run, summarize, table } from "./report"

function run(workMs: number, bytes: number): Run {
  return {
    scenario: "update-jp",
    wallMs: workMs * 2,
    buildId: "synthetic",
    resources: [],
    dataVersion: "new",
    manifestHash: "new",
    oldManifestHash: "old",
    measures: [
      { realm: "main", name: "sve:catalog:catalogOf", startMs: 0, durationMs: workMs / 2 },
      {
        realm: "worker:1:worker-test.js",
        name: "sve:catalog:ingest",
        startMs: 0,
        durationMs: workMs / 2,
      },
      {
        realm: "worker:1:worker-test.js",
        name: "sve:catalog:background:SearchIndex",
        startMs: 0,
        durationMs: 9999,
      },
    ],
    tasks: workMs > 1000 ? [{ realm: "main", name: "self", startMs: 0, durationMs: 60 }] : [],
    memory: [
      { startMs: 0, spanMs: 1, bytes, realms: 2, complete: true, isolates: [] },
      { startMs: 1, spanMs: 2, bytes: 999 * 2 ** 20, realms: 1, complete: false, isolates: [] },
    ],
  }
}

describe("catalog benchmark report", () => {
  it("takes medians of per-run totals, excludes background work and incomplete memory rounds", () => {
    const report = summarize([
      run(100, 10 * 2 ** 20),
      run(900, 30 * 2 ** 20),
      run(500, 20 * 2 ** 20),
    ])
    expect(report.workMs).toBe(500)
    expect(report.wallMs).toBe(1000)
    expect(report.peakMiB).toBe(20)
    expect(report.finalMiB).toBe(20)
    expect(report.stages["main sve:catalog:catalogOf"]).toBe(250)
    expect(report.stages["search Worker sve:catalog:ingest"]).toBe(250)
  })

  it("flags observed budget overruns without hiding a single long task behind the median", () => {
    const report: Report = {
      format: 1,
      browser: "synthetic",
      revision: "synthetic",
      dirty: false,
      recordedAt: "synthetic",
      conditions: {
        cpuQuota: "200%",
        memoryMax: "3G",
        entry: "index",
        sampleMs: 50,
        settleMs: 2000,
        workload: "synthetic",
        memoryMethod: "synthetic",
      },
      summaries: {},
      runs: [run(1200, 90 * 2 ** 20), run(800, 90 * 2 ** 20), run(1400, 90 * 2 ** 20)],
    }
    for (const entry of report.runs.slice(1)) entry.tasks = []
    const output = table(report)
    expect(output).toContain("1200.0 OVER")
    expect(output).toContain("60.0 OVER")
    expect(output).toContain("90.0 OVER")
    expect(output).toContain("do not establish mobile budget compliance")
  })
})
