/** The strictest copy limit among the restrictions, or undefined when none of them sets one. */
export function copyLimit(
  restrictions: readonly { readonly maxCopies: number | null }[] | undefined,
): number | undefined {
  const limits = (restrictions ?? []).flatMap((entry) =>
    entry.maxCopies === null ? [] : [entry.maxCopies],
  )
  return limits.length ? Math.min(...limits) : undefined
}
