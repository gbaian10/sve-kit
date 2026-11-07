import { renderHook, waitFor } from "@testing-library/react"
import { expect, it } from "vitest"

import { createScanModelStore } from "../../data/scan-models"
import { memoryCache } from "../../test-utils/cache"
import {
  fakeScanCdn,
  RUNTIMES,
  SCAN_BASE,
  scanPublication,
} from "../../test-utils/scan-publication"
import { useScanReadiness } from "./scan-hooks"

it("requires a matching runtime after a site upgrade while allowing older models", async () => {
  const storage = memoryCache()
  const cdn = fakeScanCdn()
  const options = {
    caches: () => storage,
    fetcher: cdn.fetcher,
    scanBase: () => SCAN_BASE,
    runtimes: RUNTIMES,
  }
  const store = createScanModelStore(options)
  for (const part of ["runtime-cpu", "det", "rec"] as const)
    await store.download(part, "jp", () => undefined)
  cdn.publish(scanPublication({ det: 2, rec: 2 }))
  const older = renderHook(() => useScanReadiness("jp", store))
  await waitFor(() => {
    expect(older.result.current.ready).toBe(true)
  })
  older.unmount()

  const updated = createScanModelStore({
    ...options,
    runtimes: { ...RUNTIMES, cpu: { ...RUNTIMES.cpu, sha256: "0".repeat(64) } },
  })
  const { result } = renderHook(() => useScanReadiness("jp", updated))
  await waitFor(() => {
    expect(result.current.status).not.toBeNull()
  })
  expect(result.current.ready).toBe(false)
  expect(result.current.missingBytes).toBe(RUNTIMES.cpu.bytes)
})
