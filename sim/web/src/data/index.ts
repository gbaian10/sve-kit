// Public API of the data layer: pages import from here only (lint enforces it). Exports grow with
// their first consumer, so the barrel never carries unused surface.
export {
  type AnnotatedText,
  type AnnotatedTextResolver,
  createAnnotatedTextResolver,
  peekAnnotatedTextResolver,
  type SelectedText,
  sharedAnnotatedTextResolver,
} from "./annotated-text"
export { artCredits, type FaceArtist } from "./artists"
export { appBackgroundDownloads } from "./background-downloads"
export { type CardRouteInput, locateCard, type LocatedCard } from "./card-route"
export { type CardSummary, type Catalog, catalogOf, nameOf } from "./catalog"
export {
  createSnapshotClient,
  type LoadedSnapshot,
  type SnapshotClient,
  type SnapshotStatus,
} from "./client"
export { type DigitalLink, digitalLinkUrl, loadCardDigitalLinks } from "./digital-links"
export { cardPrintings, editionPrinting, locateEdition } from "./edition"
export { type CardErratum, loadCardErrata } from "./errata"
export { faceBasics, loadFaceCandidates } from "./face-basics"
export type { Row } from "./format-v3/decode"
export { decodeMessage, type DecodeRequest } from "./format-v3/decode-message"
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
export { createLocator, GLOBAL_OWNER } from "./locator"
export { type OfficialCardLink, officialCardLinks } from "./official-card-links"
export {
  type CardRestriction,
  type CardRestrictionDetails,
  loadCardRestrictionDetails,
  type RestrictionStatus,
} from "./restrictions"
export {
  activeSnapshotRoot,
  setActiveSnapshotRoot,
  snapshotClient,
  type SnapshotRoot,
  useSnapshotRoot,
  useSnapshotStatus,
} from "./roots"
export { SearchEngine } from "./search/engine"
export type { QueryFailure, SearchOptions, SearchPage, SearchQuery } from "./search/index"
export type { SearchEvent, SearchRequest } from "./search/messages"
export {
  type SearchChannel,
  type SearchChannelStatus,
  SearchSession,
  type SearchStatus,
} from "./search/session"
export { type CompleteView, type ViewFace, viewImages } from "./search/view"
export { clearManagedCaches, estimateStorage, type StorageEstimate } from "./storage"
export { textDiff, type TextRange } from "./text-diff"
export { textRuns } from "./text-runs"
