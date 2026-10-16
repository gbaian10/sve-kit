import type { ImageSource } from "../../data"

export const shownSources = new Set<string>()

/** Foreground text cannot wait on a slow image; preloads wait for decoding before being cached. */
export async function prepareImage(
  source: ImageSource,
  sizes: string,
  preloadSignal?: AbortSignal,
): Promise<void> {
  preloadSignal?.throwIfAborted()
  const image = new Image()
  if (preloadSignal) image.fetchPriority = "low"
  image.sizes = sizes
  image.srcset = source.srcSet
  image.src = source.src
  let timer: ReturnType<typeof setTimeout> | undefined
  let cancel: (() => void) | undefined
  const decoded = image.decode().then(() => {
    if (!preloadSignal?.aborted) shownSources.add(source.src)
  })
  try {
    await Promise.race([
      preloadSignal ? decoded : decoded.catch(() => undefined),
      new Promise<void>((resolve, reject) => {
        if (preloadSignal) {
          cancel = () => {
            image.removeAttribute("srcset")
            image.removeAttribute("src")
            reject(new Error("card prefetch cancelled"))
          }
          preloadSignal.addEventListener("abort", cancel, { once: true })
        } else timer = setTimeout(resolve, 300)
      }),
    ])
    preloadSignal?.throwIfAborted()
  } finally {
    clearTimeout(timer)
    if (cancel) preloadSignal?.removeEventListener("abort", cancel)
  }
}
