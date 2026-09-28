import path from "node:path"

import js from "@eslint/js"
import type { Linter } from "eslint"
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
// Tailwind v4 also accepts a trailing `!` (important) and a `/opacity` modifier.
const IMPORTANT = String.raw`!?$`
const OPACITY_IMPORTANT = String.raw`(?:/\S+)?!?$`

const TOKEN_ONLY =
  "Colours must come from semantic tokens: add a token in src/styles, then use its class."

/** Only class strings are checked (className, cn, clsx), never test data or card text. */
const restrictedClasses = [
  { pattern: String.raw`${PREFIX}${COLOR_UTILITY}-\[${COLOR_VALUE}.*`, message: TOKEN_ONLY },
  {
    pattern: String.raw`${PREFIX}${COLOR_UTILITY}-\((?!${NON_COLOR_HINT})[^)]*\)${OPACITY_IMPORTANT}`,
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
    pattern: String.raw`${PREFIX}(?:min-|max-)?w-screen${IMPORTANT}`,
    message:
      "`w-screen` ignores the scrollbar and causes horizontal scrolling; let the container decide the width.",
  },
  {
    pattern: String.raw`${PREFIX}(?:min-|max-)?h-screen${IMPORTANT}`,
    message:
      "`h-screen` is wrong while the mobile address bar resizes; use `h-dvh` or `min-h-svh`.",
  },
  {
    pattern: String.raw`${PREFIX}(?:w|min-w|size|basis)-\[[\d.]+(?:px|rem|em|ch)\]${IMPORTANT}`,
    message:
      "Fixed arbitrary widths overflow narrow screens; use `w-full` with a `max-w-*` token, or grid/flex ratios.",
  },
]

// Fixed text with letters is UI copy, as in i18next/no-literal-string. Cooked, direct quasis only,
// so escapes (`\n`) and templates nested in `${}` do not count.
const LETTER_TEXT = String.raw`:matches(Literal[value=/\p{L}/u], TemplateLiteral:has(> TemplateElement[value.cooked=/\p{L}/u]))`
const LABEL = `Property:matches([key.name='label'], [key.value='label'])`
const PLUS = `BinaryExpression[operator='+']`
// `:has(> A > B)` missed nested operands in our esquery 1.7 tests, so nest one :has per `+`:
// four `+` levels (five operands) from the label down.
const plusWithLetterText = Array.from({ length: 3 }).reduce<string[]>(
  (levels) => [...levels, `:has(> ${PLUS}${levels.at(-1) ?? ""})`],
  [`:has(> ${LETTER_TEXT})`],
)

/** One array for every `no-restricted-syntax` entry: a later config would replace, not merge, it. */
const restrictedSyntax = {
  styleColor: {
    selector: String.raw`JSXAttribute[name.name='style'] Property:matches([key.name=/^(?:color|background|fill|stroke)$|Color$/], [key.value=/^(?:color|background|fill|stroke)$|Color$/])`,
    message: TOKEN_ONLY,
  },
  dangerousHtmlAttribute: {
    selector: `JSXAttribute[name.name='dangerouslySetInnerHTML']`,
    message: "Render card text through the markup parser instead of injecting HTML.",
  },
  dangerousHtmlProperty: {
    selector: `Property:matches([key.name='dangerouslySetInnerHTML'], [key.value='dangerouslySetInnerHTML'])`,
    message: "Render card text through the markup parser instead of injecting HTML.",
  },
  // eslint-plugin-i18next cannot check option objects outside JSX without flagging every string.
  literalLabel: {
    selector: `${LABEL} > ${LETTER_TEXT}.value`,
    message: "User-visible labels must come from t(); use t() interpolation for values.",
  },
  concatenatedLabel: {
    selector: `${LABEL} > ${PLUS}.value:matches(${plusWithLetterText.join(", ")})`,
    message: "Do not build labels by concatenation; use one t() key with interpolation.",
  },
}

const uiRestrictedSyntax = Object.values(restrictedSyntax)
// Tests and locale files legitimately hold literal labels.
const nonUiRestrictedSyntax = uiRestrictedSyntax.filter(
  (entry) =>
    entry !== restrictedSyntax.literalLabel && entry !== restrictedSyntax.concatenatedLabel,
)

const FETCH_ONLY_IN_DATA = "Only src/data/ may fetch; import its public API."
const STORAGE_ONLY_IN_SETTINGS = "Only src/settings/ may touch storage; import its public API."
const fetchBan = {
  globals: [{ name: "fetch", message: FETCH_ONLY_IN_DATA }],
  properties: ["fetch"].map((property) => ({ property, message: FETCH_ONLY_IN_DATA })),
}
const storageBan = {
  globals: ["localStorage", "sessionStorage"].map((name) => ({
    name,
    message: STORAGE_ONLY_IN_SETTINGS,
  })),
  properties: ["localStorage", "sessionStorage"].map((property) => ({
    property,
    message: STORAGE_ONLY_IN_SETTINGS,
  })),
}

/** Builds the whole rule arrays per directory, so no block half-overrides another. */
function apiBans(...bans: (typeof fetchBan)[]): Linter.RulesRecord {
  return {
    "no-restricted-globals": ["error", ...bans.flatMap((ban) => ban.globals)],
    "no-restricted-properties": [
      "error",
      ...bans.flatMap((ban) =>
        ["window", "globalThis", "self"].flatMap((object) =>
          ban.properties.map((entry) => ({ object, ...entry })),
        ),
      ),
    ],
  }
}

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
    files: ["**/*.{js,mjs,cjs}"],
    extends: [tseslint.configs.disableTypeChecked],
    languageOptions: { globals: globals.node },
  },
  // public/ holds plain browser scripts served as-is (theme-boot.js); no Node, no bundle.
  {
    files: ["public/**/*.js"],
    languageOptions: { globals: globals.browser },
  },

  {
    plugins: {
      "unused-imports": unusedImports,
      "simple-import-sort": simpleImportSort,
    },
    rules: {
      // unused-imports skips imports here, so each unused name is reported once.
      "@typescript-eslint/no-unused-vars": "off",
      "unused-imports/no-unused-imports": "error",
      "simple-import-sort/imports": "error",
      "simple-import-sort/exports": "error",
      "unused-imports/no-unused-vars": [
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
    rules: {
      ...apiBans(fetchBan, storageBan),
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
  { files: ["src/data/**"], rules: apiBans(storageBan) },
  { files: ["src/settings/**"], rules: apiBans(fetchBan) },
  {
    files: ["src/i18n/locales/**"],
    rules: { "no-restricted-syntax": ["error", ...nonUiRestrictedSyntax] },
  },

  {
    files: ["src/**/*.test.{ts,tsx}", "src/test-setup.ts", "tests/**/*.ts"],
    rules: {
      "i18next/no-literal-string": "off",
      "no-restricted-syntax": ["error", ...nonUiRestrictedSyntax],
    },
  },

  prettier,
)
