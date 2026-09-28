# sim/web

The web client: card browser, deck builder and game board in one app (React, Vite, Tailwind CSS v4, i18next).
Architecture: [`docs/sim/web-architecture.md`](../../docs/sim/web-architecture.md).

## Setup

Bun and Node.js are pinned in the repo-root `mise.toml` (`mise install`); the git hooks call them
through `mise exec`, so they do not depend on shell activation. Bun is the package manager; `bun run`
executes the Node-shebang CLIs (Vite, Vitest, ESLint, …) with Node, and Vitest's jsdom tests do not
run on the Bun runtime (see Known issues).

```bash
cd sim/web
bun install
bun run dev                     # http://localhost:5173
bun run dev -- --host 0.0.0.0   # also reachable from phones on the LAN
```

## Scripts

| Script                 | What it does                                             |
| ---------------------- | -------------------------------------------------------- |
| `bun run check`        | Everything below, in order; run it before pushing        |
| `bun run format:check` | Prettier (`bun run format` rewrites)                     |
| `bun run lint`         | ESLint (`lint:js`) and Stylelint (`lint:css`)            |
| `bun run typecheck`    | `tsc -b` with TypeScript 6                               |
| `bun run test`         | Vitest, including the ESLint and Stylelint rule fixtures |
| `bun run build`        | Production build into `dist/`                            |

`bun test` is Bun's own test runner and ignores the Vitest config. Always use `bun run test`.

The pre-commit hooks run Prettier, ESLint and Stylelint on changed files; the pre-push hook runs
the full `tsc -b`. Vitest is a manual hook (`pre-commit run --hook-stage manual web-test`); CI runs
the whole `bun run check` on every web change.

## Theme tokens

`src/styles/tokens.css` is the only file allowed colour literals. Every base colour token is
defined in three contexts: bare `:root` (light), `:root:not([data-theme="light"])` inside
`@media (prefers-color-scheme: dark)`, and `:root[data-theme="dark"]`. Accent tokens
(`--accent`, `--accent-ink`, `--accent-text`, `--accent-soft`) are defined per context for
`data-accent="amber|teal|red"`, plus a per-theme default when the attribute is absent (dark →
amber, light → teal). `tests/tokens.test.ts` parses the file and fails when a context misses a
token.

`src/styles/index.css` clears Tailwind's palette and registers only semantic colours
(`bg`, `surface-1..3`, `well`, `side`, `border`, `border-strong`, `text-1..3`, `scrim`, the
semantic `info/success/warning/danger` pairs, `class-*`, `cost`, `def`, `accent*`), the radius,
text-size and breakpoint scales (`md` 600, `lg` 1000, `xl` 1440). Class colours are used in small
areas only: put `data-class="forest"` (or any of the seven class codes) on an element and use
`border-class-current` / `text-class-current` inside it.

Which theme is active is decided by `resolveTheme()` in `src/domain/theme.ts` (a stored `theme`
of `system` and a `null` accent leave `<html>` attribute-less so the CSS falls back to the media
query and the default accent). `src/app/theme-attributes.ts` applies the attributes when the
preferences change.

**Exception to the storage rule:** `public/theme-boot.js` is a plain script loaded first in
`<head>` so a stored dark theme never flashes light. It lives outside `src/`, so the "only
`src/settings/` touches storage" lint does not cover it. It reads only the `theme` and `accent`
fields of `sve-kit:prefs`, validates them against a whitelist and swallows every error;
`src/settings/theme-boot.test.ts` runs it against good, corrupt and blocked storage and checks it
agrees with `applyThemeAttributes`.

## Settings

All user preferences live in one object stored under `localStorage["sve-kit:prefs"]`
(`src/settings/prefs.ts`): UI language, card edition, name/effect display, symbol labels, theme,
accent, ban region, data saver, grid density, view mode. Reading validates every field on its own
and falls back to the default, so a stale or hand-edited value never breaks the app; reading and
writing are wrapped in try/catch because even touching `localStorage` can throw. The object
carries `v: 1`; a missing or different version is treated as another build's data and ignored
(the boot script applies the same rule). Components subscribe with `usePrefs()`;
`prefsStore.reload()` re-reads storage. Recently viewed cards use `sve-kit:recent`
(`src/settings/recent.ts`), capped and de-duplicated on read as well as on write.

## i18n

UI text comes from `t()`. A `label` fails lint when its own fixed text (a string, or the literal
parts of a template) contains letters, or when a `+` chain up to four levels below the label has
such a string as an operand. Pure interpolation such as `` `${a}/${b}` `` passes. Deeper chains,
and text that arrives through a function or variable, are left to review.

Use one key per sentence and pass values through interpolation: `t("cost", { n })`, never
`` `${t("a")} ${n}` ``. Lint cannot catch that kind of sentence assembly, so review has to.

`src/i18n/static.test.ts` checks that the three locales have the same keys with the same
`{{placeholders}}`, and that the zh-TW file contains none of the Simplified-only characters
listed in that test (extend the list when a new one slips through review).

### Text length

Japanese and English strings are often longer than the zh-TW ones, so the layout never assumes a
length: text containers have no fixed width, and a setting row (`SettingRow`) lets its control
wrap under the label when both do not fit on one line. Controls that cannot wrap (segmented
pills) get a width budget instead: `static.test.ts` fails when any `options.*` label in any
language is wider than about 6 em, so the fix is always to shorten that language's copy, never
to widen the control. Language names use each language's own name (繁體中文 / 日本語 / English) in
every locale.

Two dev-only tools catch what the tests cannot:

- `src/app/overflow-audit.ts` logs a console error whenever a visible box is narrower than its
  content (after DOM changes, resizes and font loading), and `window.__sveOverflowAudit()` returns
  the same report on demand. The headless self-check runs every screen in all three languages and
  treats any console error as a failure.
- `?pseudo` in the dev URL stretches every string by 35% with accented letters and brackets
  (`[Çáŕð ñáḿéš ~~~ ~]`), which shows cut-off ends and squeezed rows before a real translation
  does. Its findings are advisory (the three real languages are the gate): a segmented group that
  only overflows at +35% is a warning about a fourth language, not a bug in the current three.

## Lint rule fixtures

`tests/lint-fixtures/` is a small fake project full of code that must, or must not, trip the lint
rules. Each case starts with `// case: <what> -> <rule ids | none>`; `// bypass:` marks holes the
rules knowingly cannot close. `tests/eslint-rules.test.ts` lints every file under the fixture
`src/` and checks that each case reports exactly the rules it declares, as many times as it
declares them (list a rule twice when it fires twice). Update the fixtures together with the rules.

## Dead code

`bun run knip` (part of `bun run check`) reports files, exports, types and dependencies that
nothing uses. Delete what it reports together with the UI that stopped needing it; do not keep
components or helpers "for later" — they come back from git history when their consumer lands.
The only configured exceptions are in `knip.json`: the lint rule fixtures (input data, never
imported) and the font package, which only CSS `@font-face` rules reference.
`src/i18n/usage.test.ts` does the same for locale keys: every key must appear in a source file,
either as a string literal or under a template-literal prefix such as ``t(`nav.${key}`)``.

## Known issues

- **Vitest on the Bun runtime** (Bun 1.4.2, Vitest 5.0.2, jsdom 30.1.1): `bun --bun vitest run`
  cannot start jsdom test files; jsdom throws "addEventListener called on an object that is not a
  valid instance of EventTarget". Node-environment test files pass. `bun run test` uses Node and
  passes, so do not add `--bun`.
- `bun install`, `vite build` (with and without `--bun`), DOM and async Vitest tests on Node: no
  problems found.
- `eslint-plugin-jsx-a11y` 6.10.2 declares ESLint `^9` as its peer, but works with ESLint 10 in
  the fixture tests. Its community `@types` package pulls in a second ESLint, so a local
  declaration lives in `types/`.

## Not set up yet

Added once pages exist:

- Playwright (size matrix, themes, keyboard) and `@axe-core/playwright`
- knip
- First-load size budget script
- Component tests that the jsx-a11y component mapping (`CardImage`, `Link`, `Button`) matches the rendered DOM
