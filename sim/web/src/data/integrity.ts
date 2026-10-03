import { digest } from "./format-v1/sha256"

/** Large transfer hashes run outside the UI thread when WebCrypto is available. */
export async function transferDigest(bytes: Uint8Array): Promise<string> {
  if (typeof crypto === "undefined" || !("subtle" in crypto)) return digest(bytes)
  const hash = await crypto.subtle.digest("SHA-256", bytes.slice().buffer)
  return `sha256:${Array.from(new Uint8Array(hash), (value) => value.toString(16).padStart(2, "0")).join("")}`
}
