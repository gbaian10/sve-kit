// Public API of the data layer: pages import from here only (lint enforces it). Exports grow with
// their first consumer, so the barrel never carries unused surface.
export {
  type AnnotatedText,
  type AnnotatedTextResolver,
  createAnnotatedTextResolver,
  type SelectedText,
  sharedAnnotatedTextResolver,
} from "./annotated-text"
export { locateCard, type LocatedCard } from "./card-route"
export { type CardSummary, type Catalog, catalogOf } from "./catalog"
export {
  createSnapshotClient,
  type LoadedSnapshot,
  type SnapshotClient,
  type SnapshotStatus,
} from "./client"
export type { Row } from "./format-v3/decode"
export { SnapshotError } from "./format-v3/errors"
export {
  arrayValue,
  canonicalText,
  type JsonObject,
  type JsonValue,
  objectValue,
  parseJson,
  stringValue,
} from "./format-v3/json"
export { registerImageCache } from "./image-sw-registration"
export {
  type ImageFace,
  type ImageIndex,
  type ImageSource,
  isSnapshotImageFace,
  loadImagePage,
  retainedImageIndex,
} from "./images"
export {
  activeSnapshotRoot,
  setActiveSnapshotRoot,
  snapshotClient,
  type SnapshotRoot,
  useSnapshotRoot,
  useSnapshotStatus,
} from "./roots"
export { SearchEngine } from "./search/engine"
export type { QueryFailure, SearchPage, SearchQuery } from "./search/index"
export type { SearchEvent, SearchRequest } from "./search/messages"
export {
  type SearchChannel,
  type SearchChannelStatus,
  SearchSession,
  type SearchStatus,
} from "./search/session"
export { type CompleteView, type ViewFace, viewImages } from "./search/view"
export { textRuns } from "./text-runs"
