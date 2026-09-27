const zhTW = {
  app: {
    title: "sve-kit",
    tagline: "Shadowverse: EVOLVE 非官方卡表與對戰工具",
  },
} as const

type Widen<T> = { readonly [K in keyof T]: T[K] extends string ? string : Widen<T[K]> }

/** zh-TW is the reference shape; the other locales must have exactly the same keys. */
export type Messages = Widen<typeof zhTW>

export default zhTW satisfies Messages
