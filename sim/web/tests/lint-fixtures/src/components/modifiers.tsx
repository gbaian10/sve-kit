// case: trailing important on w-screen -> better-tailwindcss/no-restricted-classes
export const WScreenImportant = () => <div className="w-screen!" />

// case: trailing important on h-screen behind a breakpoint -> better-tailwindcss/no-restricted-classes
export const HScreenImportant = () => <div className="md:h-screen!" />

// case: leading important on min-h-screen -> better-tailwindcss/no-restricted-classes
export const MinHScreenImportant = () => <div className="!min-h-screen" />

// case: trailing important on a fixed width -> better-tailwindcss/no-restricted-classes
export const FixedWidthImportant = () => <div className="w-[480px]!" />

// case: trailing important on a fixed min-width behind a breakpoint -> better-tailwindcss/no-restricted-classes
export const FixedMinWidthImportant = () => <div className="lg:min-w-[30rem]!" />

// case: css variable shorthand with important -> better-tailwindcss/no-restricted-classes
export const VarImportant = () => <div className="bg-(--raw)!" />

// case: css variable shorthand with opacity -> better-tailwindcss/no-restricted-classes
export const VarOpacity = () => <div className="bg-(--raw)/50" />

// case: css variable shorthand with opacity and important behind a breakpoint -> better-tailwindcss/no-restricted-classes
export const VarOpacityImportant = () => <div className="md:text-(--raw)/[0.3]!" />

// case: arbitrary hex with opacity and important -> better-tailwindcss/no-restricted-classes
export const HexOpacityImportant = () => <div className="bg-[#123456]/50!" />

// case: dark variant with important -> better-tailwindcss/no-restricted-classes
export const DarkImportant = () => <div className="dark:bg-surface!" />

// case: token with opacity and important -> none
export const TokenOpacityImportant = () => <div className="md:bg-surface/50!" />

// case: dynamic viewport with important -> none
export const ViewportImportant = () => <div className="h-dvh!" />

// case: non-colour shorthand with a type hint -> none
export const LengthShorthand = () => <div className="w-(length:--card-width)" />
