declare const __SVE_BUILD_ID__: string

/**
 * Hash of the code, contract schema and locked dependencies that turn verified snapshot bytes into
 * cached results. vite.config replaces the placeholder, and dev updates it after every edit.
 */
export const BUILD_ID: string = __SVE_BUILD_ID__
