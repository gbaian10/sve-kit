import type { ImageSource } from "../../data"

export const shownSources = new Set<string>()

/** Only the requested card is prepared; a slow or broken image must not hold its text hostage. */
export async function prepareImage(source: ImageSource, sizes: string): Promise<void> {
  const image = new Image()
  image.sizes = sizes
  image.srcset = source.srcSet
  image.src = source.src
  let timer: ReturnType<typeof setTimeout> | undefined
  await Promise.race([
    image
      .decode()
      .then(() => {
        shownSources.add(source.src)
      })
      .catch(() => undefined),
    new Promise<void>((resolve) => {
      timer = setTimeout(resolve, 300)
    }),
  ])
  clearTimeout(timer)
}
