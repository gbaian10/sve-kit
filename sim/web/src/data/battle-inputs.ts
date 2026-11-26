import { type RawBlobDescriptor, RawBlobStore } from "./raw-blobs"
import { appRequests } from "./request-queue"

export async function loadBattleAssets(
  base: string,
  inputs: URL,
  descriptor: RawBlobDescriptor,
): Promise<readonly [string, string]> {
  const raw = new RawBlobStore(base, appRequests.foreground, globalThis.caches)
  const [response, program] = await Promise.all([fetch(inputs), raw.read(descriptor)])
  if (!response.ok) throw new Error("format")
  return [await response.text(), new TextDecoder("utf-8", { fatal: true }).decode(program)]
}
