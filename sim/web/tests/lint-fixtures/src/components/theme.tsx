function cn(...classes: string[]) {
  return classes.join(" ")
}

// case: semantic token classes -> none
export const Tokens = () => <div className="border-accent bg-surface-1 text-text-2" />

// case: default palette was removed -> better-tailwindcss/no-unknown-classes
export const Palette = () => <div className="bg-gray-900" />

// case: palette colour inside cn() -> better-tailwindcss/no-unknown-classes
export const PaletteInCn = () => <div className={cn("p-2", "text-blue-500")} />

// case: arbitrary hex colour -> better-tailwindcss/no-restricted-classes
export const Hex = () => <div className="bg-[#123456]" />

// case: arbitrary colour with type hint -> better-tailwindcss/no-restricted-classes
export const ColorHint = () => <div className="bg-[color:var(--raw)]" />

// case: arbitrary colour function behind a variant -> better-tailwindcss/no-restricted-classes
export const Oklch = () => <div className="hover:text-[oklch(50%_0.1_200)]" />

// case: css variable shorthand -> better-tailwindcss/no-restricted-classes
export const VarShorthand = () => <div className="bg-(--raw)" />

// case: arbitrary property colour -> better-tailwindcss/no-restricted-classes
export const ArbitraryProperty = () => <div className="[color:red]" />

// case: non-colour arbitrary values -> none
export const NonColorArbitrary = () => <div className="border-[3px] bg-[url(/a.png)] text-[13px]" />

// case: dark variant -> better-tailwindcss/no-restricted-classes
export const Dark = () => <div className="dark:bg-surface-1" />

// case: stacked dark variant -> better-tailwindcss/no-restricted-classes
export const StackedDark = () => <div className="md:dark:text-text-1" />

// case: conflicting classes (reported on each side) -> better-tailwindcss/no-conflicting-classes, better-tailwindcss/no-conflicting-classes
export const Conflict = () => <div className="p-2 p-4" />

// case: style colour property -> no-restricted-syntax
export const StyleColor = () => <div style={{ backgroundColor: "red" }} />

// case: style without colour -> none
export const StyleLayout = () => <div style={{ gridTemplateColumns: "repeat(3, 1fr)" }} />

// case: class strings outside className/cn are not scanned -> none
export const Note = () => <div data-note="bg-gray-900 dark:text-text" />

// bypass: class assembled from a template literal -> none
export const Template = ({ tone }: { tone: string }) => <div className={`bg-${tone}-900`} />

// bypass: style object held in a variable -> none
const raw = { color: "red" }
export const StyleVariable = () => <div style={raw} />

// bypass: custom property inside style -> none
export const StyleCustomProperty = () => <div style={{ "--surface": "red" } as object} />
