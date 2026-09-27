// case: dynamic viewport units -> none
export const Viewport = () => <div className="h-dvh min-h-svh w-full max-w-prose" />

// case: w-screen -> better-tailwindcss/no-restricted-classes
export const WScreen = () => <div className="w-screen" />

// case: h-screen -> better-tailwindcss/no-restricted-classes
export const HScreen = () => <div className="h-screen" />

// case: min-h-screen behind a breakpoint -> better-tailwindcss/no-restricted-classes
export const MinHScreen = () => <div className="md:min-h-screen" />

// case: fixed arbitrary width -> better-tailwindcss/no-restricted-classes
export const FixedWidth = () => <div className="w-[480px]" />

// case: fixed arbitrary min-width -> better-tailwindcss/no-restricted-classes
export const FixedMinWidth = () => <div className="min-w-[30rem]" />

// case: arbitrary max-width only caps the size -> none
export const MaxWidth = () => <div className="w-full max-w-[480px]" />

// bypass: large spacing-scale width -> none
export const ScaleWidth = () => <div className="w-76" />
