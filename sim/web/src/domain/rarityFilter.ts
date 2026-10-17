export const UNRATED_RARITY = "unrated"

export function toggleRarity(
  selected: readonly string[],
  token: string,
  children: readonly string[],
): string[] {
  if (token === "pr") {
    const others = selected.filter((value) => value !== "pr" && !value.startsWith("pr:"))
    const all =
      selected.includes("pr") ||
      (children.length > 0 && children.every((value) => selected.includes(value)))
    return all ? others : [...others, "pr"].sort()
  }
  const next = new Set(selected)
  if (token.startsWith("pr:") && next.delete("pr")) for (const child of children) next.add(child)
  if (!next.delete(token)) next.add(token)
  if (children.length > 0 && children.every((child) => next.has(child))) {
    for (const value of next) if (value.startsWith("pr:")) next.delete(value)
    next.add("pr")
  }
  return [...next].sort()
}
