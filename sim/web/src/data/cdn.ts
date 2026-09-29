export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>

/** A transport failure: the bytes never arrived intact, so the same request may succeed later. */
export class NetworkError extends Error {
  readonly status: number | undefined

  constructor(message: string, status?: number) {
    super(message)
    this.name = "NetworkError"
    this.status = status
  }
}

export async function fetchBytes(
  fetcher: Fetcher,
  url: string,
  init?: RequestInit,
): Promise<Uint8Array> {
  let response: Response
  try {
    response = await fetcher(url, init)
  } catch (error) {
    throw new NetworkError(error instanceof Error ? error.message : "fetch failed")
  }
  if (!response.ok)
    throw new NetworkError(`HTTP ${String(response.status)} for ${url}`, response.status)
  // A body that stops arriving is the same transport failure as a request that never connected.
  try {
    return new Uint8Array(await response.arrayBuffer())
  } catch (error) {
    throw new NetworkError(error instanceof Error ? error.message : "response body failed")
  }
}
