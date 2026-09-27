import path from "node:path"

import js from "@eslint/js"
import { defineConfig, globalIgnores } from "eslint/config"
import prettier from "eslint-config-prettier/flat"
import betterTailwind from "eslint-plugin-better-tailwindcss"
import i18next from "eslint-plugin-i18next"
import jsxA11y from "eslint-plugin-jsx-a11y"
import reactHooks from "eslint-plugin-react-hooks"
import reactRefresh from "eslint-plugin-react-refresh"
import simpleImportSort from "eslint-plugin-simple-import-sort"
import unusedImports from "eslint-plugin-unused-imports"
import globals from "globals"
import tseslint from "typescript-eslint"

const root = import.meta.dirname

// Variant / important / negative prefix in front of a utility, e.g. `md:hover:!-`.
const PREFIX = String.raw`^(?:.*:)?!?-?`
const COLOR_UTILITY = String.raw`(?:bg|text|border(?:-[xytrblse])?|outline|ring(?:-offset)?|inset-ring|divide|fill|stroke|decoration|accent|caret|shadow|inset-shadow|drop-shadow|text-shadow|placeholder|from|via|to)`
const COLOR_VALUE = String.raw`(?:#|color:|--|(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|color-mix|light-dark|var)\(|[a-z]+\])`
const NON_COLOR_HINT = String.raw`(?:length|number|percentage|integer|ratio|angle|image|url|position|bg-size|line-width|family-name|absolute-size|relative-size):`

const TOKEN_ONLY =
  "Colours must come from semantic tokens: add a token in src/styles, then use its class."

/** Only class strings are checked (className, cn, clsx), never test data or card text. */
const restrictedClasses = [
  { pattern: String.raw`${PREFIX}${COLOR_UTILITY}-\[${COLOR_VALUE}.*`, message: TOKEN_ONLY },
  {
    pattern: String.raw`${PREFIX}${COLOR_UTILITY}-\((?!${NON_COLOR_HINT}).*\)$`,
    message: TOKEN_ONLY,
  },
  {
    pattern: String.raw`${PREFIX}\[[a-z-]*(?:color|background|fill|stroke|shadow)[a-z-]*:.*`,
    message: TOKEN_ONLY,
  },
  {
    pattern: String.raw`^(?:.*:)?dark:.*`,
    message: "Tokens already switch per theme; `dark:` would keep a second palette by hand.",
  },
  {
    pattern: String.raw`${PREFIX}(?:min-|max-)?w-screen$`,
    message:
      "`w-screen` ignores the scrollbar and causes horizontal scrolling; let the container decide the width.",
  },
  {
    pattern: String.raw`${PREFIX}(?:min-|max-)?h-screen$`,
    message:
      "`h-screen` is wrong while the mobile address bar resizes; use `h-dvh` or `min-h-svh`.",
  },
  {
    pattern: String.raw`${PREFIX}(?:w|min-w|size|basis)-\[[\d.]+(?:px|rem|em|ch)\]$`,
    message:
      "Fixed arbitrary widths overflow narrow screens; use `w-full` with a `max-w-*` token, or grid/flex ratios.",
  },
]

/** One array for every `no-restricted-syntax` entry: a later config would replace, not merge, it. */
const restrictedSyntax = {
  styleColor: {
    selector: String.raw`JSXAttribute[name.name='style'] Property[key.name=/^(?:color|background|fill|stroke)$|Color$/]`,
    message: TOKEN_ONLY,
  },
  dangerousHtmlAttribute: {
    selector: `JSXAttribute[name.name='dangerouslySetInnerHTML']`,
    message: "Render card text through the markup parser instead of injecting HTML.",
  },
  dangerousHtmlProperty: {
    selector: `Property[key.name='dangerouslySetInnerHTML']`,
    message: "Render card text through the markup parser instead of injecting HTML.",
  },
  // eslint-plugin-i18next cannot check option objects outside JSX without flagging every string.
  literalLabel: {
    selector: String.raw`Property[key.name='label'] > Literal[value=/\S/]`,
    message: "User-visible labels must come from t().",
  },
}

const uiRestrictedSyntax = Object.values(restrictedSyntax)
const testRestrictedSyntax = uiRestrictedSyntax.filter(
  (entry) => entry !== restrictedSyntax.literalLabel,
)

export default defineConfig(
  globalIgnores(["dist/", "node_modules/", "tests/lint-fixtures/"]),

  js.configs.recommended,
  tseslint.configs.strictTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: root,
      },
    },
  },
  {
    files: ["**/*.js"],
    extends: [tseslint.configs.disableTypeChecked],
    languageOptions: { globals: globals.node },
  },

  {
    plugins: {
      "unused-imports": unusedImports,
      "simple-import-sort": simpleImportSort,
    },
    rules: {
      "unused-imports/no-unused-imports": "error",
      "simple-import-sort/imports": "error",
      "simple-import-sort/exports": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrors: "none" },
      ],
      "no-console": ["error", { allow: ["warn", "error"] }],
    },
  },

  {
    files: ["src/**/*.{ts,tsx}"],
    extends: [
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
      jsxA11y.flatConfigs.strict,
    ],
    plugins: { "better-tailwindcss": betterTailwind, i18next },
    languageOptions: { globals: globals.browser },
    settings: {
      "jsx-a11y": {
        components: { CardImage: "img", Link: "a", Button: "button" },
      },
      "better-tailwindcss": {
        entryPoint: path.join(root, "src/styles/index.css"),
        selectors: [
          { kind: "attribute", name: "^className$", match: [{ type: "strings" }] },
          {
            kind: "callee",
            name: "^(?:cn|clsx)$",
            match: [{ type: "strings" }, { type: "objectKeys" }],
          },
        ],
      },
    },
    rules: {
      "better-tailwindcss/no-unknown-classes": "error",
      "better-tailwindcss/no-conflicting-classes": "error",
      "better-tailwindcss/no-restricted-classes": ["error", { restrict: restrictedClasses }],
      "i18next/no-literal-string": [
        "error",
        {
          mode: "jsx-only",
          "jsx-attributes": {
            include: ["aria-label", "aria-description", "placeholder", "title", "alt", "label"],
          },
        },
      ],
      "no-restricted-syntax": ["error", ...uiRestrictedSyntax],
    },
  },

  {
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["src/data/**", "src/settings/**"],
    rules: {
      "no-restricted-globals": [
        "error",
        { name: "fetch", message: "Only src/data/ may fetch; import its public API." },
        { name: "localStorage", message: "Only src/settings/ may touch storage." },
        { name: "sessionStorage", message: "Only src/settings/ may touch storage." },
      ],
      "no-restricted-properties": [
        "error",
        ...["window", "globalThis", "self"].flatMap((object) => [
          { object, property: "fetch", message: "Only src/data/ may fetch." },
          { object, property: "localStorage", message: "Only src/settings/ may touch storage." },
          { object, property: "sessionStorage", message: "Only src/settings/ may touch storage." },
        ]),
      ],
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: String.raw`(?:^|/)(?:data|settings)/.+`,
              message: "Import src/data or src/settings through their index, not internal files.",
            },
          ],
        },
      ],
    },
  },

  {
    files: ["src/**/*.test.{ts,tsx}", "src/test-setup.ts", "tests/**/*.ts"],
    rules: {
      "i18next/no-literal-string": "off",
      "no-restricted-syntax": ["error", ...testRestrictedSyntax],
    },
  },

  prettier,
)
