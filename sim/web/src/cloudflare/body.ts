/** The body was not UTF-8 JSON, or was longer than allowed: the sender's fault, not the network's. */
export class BodyFormatError extends Error {}

/**
 * Reads a JSON body of at most `maxBytes`. A stream that fails (a timeout, a dropped connection)
 * rejects with its own error; bad bytes, bad JSON or too many bytes reject with `BodyFormatError`.
 */
export async function readJsonBody(
  body: ReadableStream<Uint8Array> | null,
  maxBytes: number,
): Promise<unknown> {
  const reader = body?.getReader()
  if (!reader) throw new BodyFormatError("No body")
  const buffer = new Uint8Array(maxBytes)
  let size = 0
  for (;;) {
    const { value, done } = await reader.read()
    if (done) break
    if (size + value.byteLength > maxBytes) {
      await reader.cancel()
      throw new BodyFormatError("Body too large")
    }
    buffer.set(value, size)
    size += value.byteLength
  }
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(buffer.subarray(0, size)))
  } catch (cause) {
    throw new BodyFormatError("Body is not UTF-8 JSON", { cause })
  }
}
