/**
 * The text of a source link. The URL is already restricted to official hosts and accounts, so the
 * label only names where it points: the account for an X post, the service for a video, else the host.
 */
export function referenceLabel(url: string): string {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return url
  }
  const host = parsed.hostname.replace(/^www\./u, "")
  if (host === "x.com" || host === "twitter.com") {
    const account = parsed.pathname.split("/")[1]
    return account ? `X @${account}` : "X"
  }
  if (host === "youtube.com" || host === "youtu.be") return "YouTube"
  return host
}
