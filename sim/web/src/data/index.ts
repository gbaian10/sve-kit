// Public API of the data layer: pages import from here only (lint enforces it). Exports grow with
// their first consumer, so the barrel never carries unused surface.
export { type CardSummary, type Catalog, catalogOf } from "./catalog"
export {
  createSnapshotClient,
  type LoadedSnapshot,
  type SnapshotClient,
  type SnapshotStatus,
} from "./client"
export type { Row } from "./format-v1/decode"
export { registerImageCache } from "./image-sw-registration"
export { type ImageFace, type ImageIndex, loadImagePage } from "./images"
export {
  activeSnapshotRoot,
  setActiveSnapshotRoot,
  snapshotClient,
  type SnapshotRoot,
  useSnapshotRoot,
  useSnapshotStatus,
} from "./roots"
