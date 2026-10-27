import type { PostProcessorModule } from "i18next"

import { pseudoLocalize } from "./pseudo-localize"

/** i18next post-processor; enabled in dev with `?pseudo` in the URL. */
export const pseudoPostProcessor: PostProcessorModule = {
  type: "postProcessor",
  name: "pseudo",
  process: (value: string) => pseudoLocalize(value),
}
