interface ImportMetaEnv {
  /** Snapshot root the built app reads; dev defaults to the Vite `/cdn` middleware. */
  readonly VITE_CDN_BASE?: string
  /** Injected by vite.config: "1" when SVE_PREVIEW_DIR was set for this dev server. */
  readonly SVE_PREVIEW_CONFIGURED?: string
}
