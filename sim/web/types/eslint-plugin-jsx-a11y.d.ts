// The package ships no types, and @types/eslint-plugin-jsx-a11y drags in a second ESLint (v9).
declare module "eslint-plugin-jsx-a11y" {
  import type { ESLint, Linter } from "eslint"

  const plugin: ESLint.Plugin & {
    flatConfigs: { recommended: Linter.Config; strict: Linter.Config }
  }
  export default plugin
}
