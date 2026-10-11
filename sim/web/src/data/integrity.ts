/** Hash exact downloaded or cached bytes with the browser’s native SHA-256 implementation. */
export async function transferDigest(bytes: Uint8Array): Promise<string> {
  // Only a view of a shared buffer needs a private copy; WebCrypto reads other views directly.
  const data =
    bytes.buffer instanceof ArrayBuffer
      ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength)
      : bytes.slice()
  const hash = await crypto.subtle.digest("SHA-256", data)
  return `sha256:${Array.from(new Uint8Array(hash), (value) => value.toString(16).padStart(2, "0")).join("")}`
}
