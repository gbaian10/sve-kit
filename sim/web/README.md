# sim/web

The web client: card browser, deck builder and game board in one app (React, Vite, Tailwind CSS v4, i18next).

## Setup

Bun is pinned in the repo-root `mise.toml` (`mise install`). Node.js must also be on `PATH`:
`bun run` executes the Node-shebang CLIs (Vite, Vitest, ESLint, …) with Node.

```bash
cd sim/web
bun install
bun run dev
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

The pre-commit hooks run Prettier, ESLint and Stylelint on changed files; the pre-push hooks run
the full `tsc -b` and Vitest.

## Lint rule fixtures

`tests/lint-fixtures/` is a small fake project full of code that must, or must not, trip the lint
rules. Each case starts with `// case: <what> -> <rule ids | none>`; `// bypass:` marks holes the
rules knowingly cannot close. `tests/eslint-rules.test.ts` checks that every case reports exactly
the rules it declares. Update the fixtures together with the rules.

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
- i18n static check (keys and interpolation across the three locales, Simplified-only characters in zh-TW)
- Theme token sync check (every managed colour token defined in all three theme selectors)
- Component tests that the jsx-a11y component mapping (`CardImage`, `Link`, `Button`) matches the rendered DOM
