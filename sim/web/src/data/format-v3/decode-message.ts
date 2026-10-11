import { type JsonObject, objectValue, parseJson } from "./json"
import { type Fragment, readContainer, readManifest } from "./reader"

export type DecodeRequest =
  | { readonly kind: "manifest"; readonly bytes: Uint8Array }
  | {
      readonly kind: "file"
      readonly bytes: Uint8Array
      readonly file: JsonObject
      readonly version: string
    }
export type DecodeResult =
  | {
      readonly kind: "manifest"
      readonly manifest: JsonObject
      readonly files: Map<string, JsonObject>
    }
  | { readonly kind: "config"; readonly value: JsonObject }
  | { readonly kind: "fragments"; readonly fragments: readonly Fragment[] }

/** Decodes published bytes after their caller has verified the content hash. */
export function decodeMessage(request: DecodeRequest): DecodeResult {
  if (request.kind === "manifest")
    return { kind: "manifest", ...readManifest(parseJson(request.bytes)) }
  const value = objectValue(parseJson(request.bytes))
  if (request.file["role"] === "config") {
    return { kind: "config", value }
  }
  const fragments = readContainer(request.file, value, request.version)
  return { kind: "fragments", fragments }
}
