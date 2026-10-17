import type { Measure, MemorySample } from "./monitor"

export interface Run {
  scenario: string
  wallMs: number
  buildId: string
  resources: { durationMs: number; transferBytes: number; decodedBytes: number }[]
  dataVersion: string
  manifestHash: string
  oldManifestHash?: string
  measures: Measure[]
  tasks: Measure[]
  memory: MemorySample[]
}
export interface Report {
  format: 1
  recordedAt: string
  browser: string
  revision: string
  dirty: boolean
  summaries: Record<string, ReturnType<typeof summarize>>
  conditions: {
    cpuQuota: string
    memoryMax: string
    entry: string
    sampleMs: number
    settleMs: number
    workload: string
    memoryMethod: string
  }
  runs: Run[]
}

function median(values: readonly number[]): number {
  if (!values.length) throw new Error("No samples")
  const ordered = [...values].sort((a, b) => a - b)
  const middle = Math.floor(ordered.length / 2)
  return ordered.length % 2
    ? (ordered[middle] ?? 0)
    : ((ordered[middle - 1] ?? 0) + (ordered[middle] ?? 0)) / 2
}

export const realmGroup = (realm: string) =>
  realm === "main" ? "main" : realm.includes("decode-worker-") ? "decode Worker" : "search Worker"

export function summarize(runs: readonly Run[]) {
  const mid = (read: (run: Run) => number) => median(runs.map(read))
  const stages = [
    ...new Set(
      runs.flatMap((run) =>
        run.measures.map((entry) => `${realmGroup(entry.realm)} ${entry.name}`),
      ),
    ),
  ].sort()
  return {
    runs: runs.length,
    wallMs: mid((run) => run.wallMs),
    workMs: mid((run) =>
      run.measures
        .filter((entry) => !entry.name.includes(":background:"))
        .reduce((sum, entry) => sum + entry.durationMs, 0),
    ),
    longTasks: mid((run) => run.tasks.length),
    longestTaskMs: mid((run) => Math.max(0, ...run.tasks.map((entry) => entry.durationMs))),
    worstTaskMs: Math.max(0, ...runs.flatMap((run) => run.tasks.map((entry) => entry.durationMs))),
    maxSampleSpanMs: Math.max(0, ...runs.flatMap((run) => run.memory.map((entry) => entry.spanMs))),
    networkRequestMs: mid((run) => run.resources.reduce((sum, entry) => sum + entry.durationMs, 0)),
    peakMiB: mid(
      (run) =>
        Math.max(...run.memory.filter((entry) => entry.complete).map((entry) => entry.bytes)) /
        2 ** 20,
    ),
    finalMiB: mid(
      (run) => (run.memory.filter((entry) => entry.complete).at(-1)?.bytes ?? 0) / 2 ** 20,
    ),
    stages: Object.fromEntries(
      stages.map((stage) => [
        stage,
        mid((run) =>
          run.measures
            .filter((entry) => `${realmGroup(entry.realm)} ${entry.name}` === stage)
            .reduce((sum, entry) => sum + entry.durationMs, 0),
        ),
      ]),
    ),
  }
}

export function table(report: Report): string {
  const rows = [
    "Scenario | n | work ms / 1000 | worst task ms / 50 | long tasks | sampled peak MiB (update / 80) | final MiB / 48 | wall ms",
    "--- | ---: | ---: | ---: | ---: | ---: | ---: | ---:",
  ]
  const stages: string[] = []
  const cell = (value: number, limit: number) =>
    `${value.toFixed(1)}${value > limit ? " OVER" : " *"}`
  for (const scenario of new Set(report.runs.map((run) => run.scenario))) {
    const summary = summarize(report.runs.filter((run) => run.scenario === scenario))
    rows.push(
      `${scenario} | ${String(summary.runs)} | ${cell(summary.workMs, 1000)} | ${cell(summary.worstTaskMs, 50)} | ${String(summary.longTasks)} | ${scenario.startsWith("update") ? cell(summary.peakMiB, 80) : summary.peakMiB.toFixed(1)} | ${cell(summary.finalMiB, 48)} | ${summary.wallMs.toFixed(1)}`,
    )
    stages.push("", `${scenario}: median stage totals (ms; by realm)`)
    for (const [stage, value] of Object.entries(summary.stages))
      stages.push(`  ${stage}: ${value.toFixed(2)}`)
    stages.push("")
  }
  rows.push(...stages)
  rows.push(
    "* Within the observed proxy only; desktop samples do not establish mobile budget compliance.",
    "Stage totals are non-nested effective work and exclude background:*; wall includes fetch/cache/hash/waits. Peak is gross JS heap + backing storage,",
    "sampled across isolates with recorded sampling spans; final is an unforced-GC sample, not proven steady state.",
  )
  return rows.join("\n")
}
