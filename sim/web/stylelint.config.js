const TAILWIND_AT_RULES = [
  "theme",
  "utility",
  "variant",
  "custom-variant",
  "source",
  "apply",
  "reference",
  "plugin",
  "config",
]

/** @type {import('stylelint').Config} */
export default {
  extends: ["stylelint-config-standard"],
  rules: {
    "at-rule-no-unknown": [true, { ignoreAtRules: TAILWIND_AT_RULES }],
    "at-rule-no-deprecated": [true, { ignoreAtRules: ["apply"] }],
    "function-no-unknown": [true, { ignoreFunctions: ["theme", "--alpha", "--spacing"] }],
    "import-notation": "string",
    // `--color-*: initial` is how Tailwind v4 clears a theme namespace.
    "custom-property-pattern": [
      String.raw`^[a-z][a-z0-9]*(-[a-z0-9]+)*(-\*)?$`,
      { message: "Custom properties must be kebab-case." },
    ],
    "color-no-hex": true,
    "color-named": "never",
    "function-disallowed-list": [
      [
        "rgb",
        "rgba",
        "hsl",
        "hsla",
        "hwb",
        "lab",
        "lch",
        "oklab",
        "oklch",
        "color",
        "color-mix",
        "light-dark",
      ],
      { message: "Colours belong in src/styles/tokens.css; reference a token instead." },
    ],
    "unit-disallowed-list": [
      ["vh"],
      { message: "`vh` is wrong while the mobile address bar resizes; use `dvh` or `svh`." },
    ],
    "declaration-no-important": true,
  },
  overrides: [
    {
      files: ["src/styles/tokens.css"],
      rules: {
        "color-no-hex": null,
        "color-named": null,
        "function-disallowed-list": null,
      },
    },
  ],
}
