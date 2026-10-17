import { createSnapshotClient, SearchSession } from "../src/data"
import { BUILD_ID } from "../src/data/build-id"
import type { Region } from "../src/domain/search"

const entry = new URL(location.href).searchParams.get("entry") === "preview" ? "preview" : "index"
const client = createSnapshotClient("/cdn", { entry })
const session = new SearchSession()

async function run(edition: Region, update: boolean) {
  const before = session.getStatus().generation
  const searchable = new Promise<void>((resolve, reject) => {
    const unsubscribe = session.subscribe(() => {
      const status = session.getStatus()
      if (status.error) {
        unsubscribe()
        reject(new Error(status.error.message))
      } else if (
        status.phase === "ready" &&
        status.searchReady &&
        status.generation !== before &&
        !status.updating
      ) {
        unsubscribe()
        resolve()
      }
    })
  })
  const start = performance.now()
  session.load("/cdn", edition, entry)
  await Promise.all([searchable, update ? client.reload() : client.selectRegion?.(edition)])
  const status = client.status()
  if (status.state !== "ready" || status.updateError) throw new Error("Main catalog failed")
  const snapshot = client.snapshot()
  const search = session.getStatus()
  if (!snapshot || snapshot.manifestHash !== search.manifestHash)
    throw new Error("Main and search Worker adopted different snapshots")
  return {
    wallMs: performance.now() - start,
    buildId: BUILD_ID,
    resources: performance
      .getEntriesByType("resource")
      .filter(
        (entry): entry is PerformanceResourceTiming =>
          entry instanceof PerformanceResourceTiming &&
          entry.name.includes("/cdn/") &&
          entry.startTime >= start,
      )
      .map((entry) => ({
        durationMs: entry.duration,
        transferBytes: entry.transferSize,
        decodedBytes: entry.decodedBodySize,
      })),
    dataVersion: snapshot.dataVersion,
    manifestHash: snapshot.manifestHash,
  }
}

Object.assign(globalThis, { catalogBenchmark: { run } })
