import type { Region } from "../../domain/search"
import type { JsonObject } from "../format-v3/json"
import type { Files } from "../format-v3/reader"
import type { SearchPage, SearchQuery } from "./index"

export interface LoadRequest {
  readonly kind: "load"
  readonly generation: number
  readonly base: string
  readonly edition: Region
  readonly entry: "index" | "preview"
}
export type SearchRequest =
  | LoadRequest
  | { readonly kind: "cancel"; readonly generation: number }
  | {
      readonly kind: "search"
      readonly generation: number
      readonly queryId: number
      readonly query: SearchQuery
    }

export interface SearchMetrics {
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
}
export type SearchEvent =
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
      readonly dataVersion: string
      readonly manifestHash: string
      readonly metrics: SearchMetrics
    }
  | {
      readonly kind: "result"
      readonly generation: number
      readonly queryId: number
      readonly page: SearchPage
    }
  | {
      readonly kind: "error"
      readonly generation: number
      readonly queryId?: number
      readonly message: string
    }

/** Packaging can change without changing how the worker stages and publishes a generation. */
export type BootstrapPlan = (
  manifest: JsonObject,
  files: Files,
  edition: Region,
) => readonly string[]
