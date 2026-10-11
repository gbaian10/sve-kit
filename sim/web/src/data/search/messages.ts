import type { Region } from "../../domain/search"
import type { JsonObject } from "../format-v3/json"
import type { Files } from "../format-v3/reader"
import type {
  QueryFailure,
  RecentSummary,
  SearchPage,
  SearchQuery,
  SuggestedSummary,
} from "./index"
import type { CompleteView, ViewFace } from "./view"

export interface LoadRequest {
  readonly kind: "load"
  readonly generation: number
  readonly base: string
  readonly edition: Region
  readonly entry: "index" | "preview"
}
export type PurposeRequest =
  | { readonly kind: "search"; readonly query: SearchQuery }
  | { readonly kind: "view"; readonly query: SearchQuery; readonly remember?: boolean }
  | { readonly kind: "suggest"; readonly text: string; readonly limit: number }
  | { readonly kind: "recent"; readonly printingIds: readonly string[] }
export type QueryRequest = PurposeRequest & {
  readonly generation: number
  readonly channelId: number
  readonly queryId: number
}
export type SearchRequest =
  | LoadRequest
  | { readonly kind: "close-channel"; readonly channelId: number }
  | { readonly kind: "cancel"; readonly generation: number }
  | QueryRequest

export interface SearchMetrics {
  /** The index came from the persistent copy of an earlier verified build; nothing was decoded. */
  readonly restored: boolean
  readonly cacheReadMs?: number
  readonly digestMs?: number
  readonly files: number
  readonly maxRawBytes: number
  readonly parseMs: number
  readonly buildMs: number
  readonly wallMs: number
  readonly firstFileMs: number
  readonly firstSetMs: number
  readonly arrayBuffers: number
  readonly stringBytes: number
  readonly cards: number
  readonly stagingArrayBuffers: number
  readonly stagingStringBytes: number
}
export type PurposeResult =
  | { readonly kind: "view-result"; readonly view: CompleteView }
  | {
      readonly kind: "suggest-result"
      readonly total: number
      readonly items: readonly SuggestedSummary[]
      readonly faces?: readonly ViewFace[]
    }
  | {
      readonly kind: "recent-result"
      readonly items: readonly RecentSummary[]
      readonly faces?: readonly ViewFace[]
    }
export type SearchStage = "index" | "manifest" | "config" | "bootstrap" | "search" | "images"

type WorkerEvent =
  | { readonly kind: "stage"; readonly generation: number; readonly step: SearchStage }
  | (PurposeResult & {
      readonly generation: number
      readonly channelId: number
      readonly queryId: number
    })
  | { readonly kind: "checking"; readonly generation: number; readonly error?: string }
  | {
      readonly kind: "progress"
      readonly generation: number
      readonly done: number
      readonly total: number
      readonly persistent: boolean
    }
  | {
      readonly kind: "ready"
      readonly generation: number
      readonly edition: Region
      readonly root: string
      readonly dataVersion: string
      readonly manifestHash: string
      readonly metrics: SearchMetrics
      readonly searchReady?: boolean
      readonly unconfirmed?: boolean
    }
  | {
      readonly kind: "result"
      readonly generation: number
      readonly channelId: number
      readonly queryId: number
      readonly page: SearchPage
    }
  | {
      readonly kind: "error"
      readonly generation: number
      readonly queryId?: never
      readonly message: string
    }
  | {
      readonly kind: "query-error"
      readonly generation: number
      readonly channelId: number
      readonly queryId: number
      readonly error: QueryFailure
    }

export type SearchEvent = WorkerEvent & { readonly sentAt?: number }

/** Packaging can change without changing how the worker stages and publishes a generation. */
export type BootstrapPlan = (
  manifest: JsonObject,
  files: Files,
  edition: Region,
) => readonly string[]
