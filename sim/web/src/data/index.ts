// Public API of the data layer: pages import from here only (lint enforces it). Exports grow with
// their first consumer, so the barrel never carries unused surface.
export type { SnapshotClient, SnapshotStatus } from "./client"
export {
  activeSnapshotRoot,
  setActiveSnapshotRoot,
  snapshotClient,
  type SnapshotRoot,
  useSnapshotRoot,
  useSnapshotStatus,
} from "./roots"
