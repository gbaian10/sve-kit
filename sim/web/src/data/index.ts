// Public API of the data layer: pages import from here only (lint enforces it). Exports grow with
// their first consumer, so the barrel never carries unused surface.
export {
  type CardFaceView,
  type CardView,
  createRouteLookups,
  type KeywordInfo,
  loadCardView,
} from "./cardView"
export { type CardSummary, type Catalog, catalogOf, type FilterOptions } from "./catalog"
export type { LoadedSnapshot, SnapshotClient, SnapshotStatus } from "./client"
export { globalDetailOf } from "./detail"
export { type ImageIndex, imageIndexOf } from "./images"
export {
  activeSnapshotRoot,
  setActiveSnapshotRoot,
  snapshotClient,
  type SnapshotRoot,
  useSnapshotRoot,
  useSnapshotStatus,
} from "./roots"
