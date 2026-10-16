import type { ImageSource } from "../../data"

export const shownSources = new Set<string>()

/** A slow or broken image must not hold the requested card's text hostage. */
export async function prepareImage(
  source: ImageSource,
  sizes: string,
  signal?: AbortSignal,
): Promise<void> {
  signal?.throwIfAborted()
  const image = new Image()
  if (signal) image.fetchPriority = "low"
  image.sizes = sizes
  image.srcset = source.srcSet
  image.src = source.src
  let timer: ReturnType<typeof setTimeout> | undefined
  let cancel: (() => void) | undefined
  const decoded = image.decode().then(() => {
    if (!signal?.aborted) shownSources.add(source.src)
  })
  try {
    await Promise.race([
      signal ? decoded : decoded.catch(() => undefined),
      new Promise<void>((resolve, reject) => {
        if (signal) {
          cancel = () => {
            image.removeAttribute("srcset")
            image.removeAttribute("src")
            reject(new Error("card prefetch cancelled"))
          }
          signal.addEventListener("abort", cancel, { once: true })
        } else timer = setTimeout(resolve, 300)
      }),
    ])
    signal?.throwIfAborted()
  } finally {
    clearTimeout(timer)
    if (cancel) signal?.removeEventListener("abort", cancel)
  }
}
