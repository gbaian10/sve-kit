import type { SnapshotRoot } from "../data"

// Prefer configured real preview data over the fixture without overriding a real CDN root.
export function initialSnapshotRoot(env: {
  readonly DEV: boolean
  readonly SVE_PREVIEW_CONFIGURED?: string
  readonly SVE_CDN_IS_FIXTURE?: string
}): SnapshotRoot {
  return env.DEV && env.SVE_PREVIEW_CONFIGURED === "1" && env.SVE_CDN_IS_FIXTURE === "1"
    ? "preview"
    : "cdn"
}
