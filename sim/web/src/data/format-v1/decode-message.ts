import { type JsonObject, objectValue, parseStrict, stringValue } from "./json"
import { type Fragment, readContainer, readPayload, verifyManifest } from "./reader"
import { validate } from "./schema"
import { validateConfig, validateFragments } from "./semantics"

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

/** Same decoder in browsers, fixture scripts and tests; workers do not weaken validation. */
export function decodeMessage(request: DecodeRequest): DecodeResult {
  if (request.kind === "manifest")
    return { kind: "manifest", ...verifyManifest(parseStrict(request.bytes)) }
  const value = objectValue(readPayload(request.file, request.bytes))
  if (request.file["role"] === "config") {
    validate("Config", value, [stringValue(request.file["key"])], request.version)
    validateConfig(value)
    return { kind: "config", value }
  }
  const fragments = readContainer(request.file, value, request.version)
  validateFragments(fragments)
  return { kind: "fragments", fragments }
}
