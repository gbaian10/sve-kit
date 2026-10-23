// Keep Prettier for editor integration until Bun provides one.
// Match root bunfig.toml so saving and committing produce the same formatting.
/** @type {import('prettier').Config & import('prettier-plugin-tailwindcss').PluginOptions} */
export default {
  semi: false,
  printWidth: 100,
  plugins: ["prettier-plugin-tailwindcss"],
  tailwindStylesheet: "./src/styles/index.css",
  tailwindFunctions: ["cn", "clsx"],
}
