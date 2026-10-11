import { digest } from "./format-v3/sha256"

/** WebCrypto is far faster than the JS fallback, although Chromium still hashes on the calling thread. */
export async function transferDigest(bytes: Uint8Array): Promise<string> {
  if (typeof crypto === "undefined" || !("subtle" in crypto)) return digest(bytes)
  // Only a view of a shared buffer needs a private copy; WebCrypto reads other views directly.
  const data =
    bytes.buffer instanceof ArrayBuffer
      ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength)
      : bytes.slice()
  const hash = await crypto.subtle.digest("SHA-256", data)
  return `sha256:${Array.from(new Uint8Array(hash), (value) => value.toString(16).padStart(2, "0")).join("")}`
}
