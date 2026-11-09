// Public API of the data layer: pages import from here only (lint enforces it). Pending exports
// remain tagged until their consumer is confirmed or the maintainer removes them.
export {
  type AnnotatedText,
  type AnnotatedTextResolver,
  peekAnnotatedTextResolver,
  type SelectedText,
  sharedAnnotatedTextResolver,
} from "./annotated-text"
export { artCredits, type FaceArtist } from "./artists"
export { appBackgroundDownloads } from "./background-downloads"
export { type CardRouteInput, locateCard, type LocatedCard } from "./card-route"
export {
  type CardSummary,
  type Catalog,
  catalogOf,
  isUnlistedPrinting,
  nameOf,
  printingTypeCode,
} from "./catalog"
export { type LoadedSnapshot, type SnapshotClient, type SnapshotStatus } from "./client"
export { loadConstructionIndex } from "./construction-index"
export { type DigitalLink, digitalLinkUrl, loadCardDigitalLinks } from "./digital-links"
export { cardPrintings, locateEdition } from "./edition"
export { type CardErratum, loadCardErrata } from "./errata"
export { faceBasics, loadFaceCandidates } from "./face-basics"
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
  mergeImageIndexes,
  retainedImageIndex,
} from "./images"
export { createLocator, GLOBAL_OWNER } from "./locator"
export { type OfficialCardLink, officialCardLinks } from "./official-card-links"
export {
  type CardRestriction,
  type CardRestrictionDetails,
  loadCardRestrictionDetails,
  loadRestrictionSelection,
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
export { appScanModels, loadScanCardImage, scanCardImage } from "./scan"
export {
  type InstalledScanModels,
  runtimePart,
  SCAN_PARTS,
  ScanModelError,
  type ScanModelErrorKind,
  type ScanModelsStatus,
  type ScanModelStore,
  type ScanPart,
  type ScanPartStatus,
} from "./scan-models"
export { SCAN_BACKENDS, type ScanBackend } from "./scan-runtime"
export type { QueryFailure, SearchOptions, SearchPage, SearchQuery } from "./search/index"
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
