# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository Overview

This is a monorepo for **solid-jsx-oxc**, a Rust + OXC port of `babel-plugin-jsx-dom-expressions` that compiles SolidJS JSX to optimized DOM/SSR runtime calls. The native compiler is exposed to Node.js via NAPI-RS and wrapped by Vite/Rolldown/Bun plugins.

The project's stated compatibility goal is to be a drop-in replacement for `babel-plugin-jsx-dom-expressions` in common SolidJS setups. Gaps and deferred features are tracked in `packages/solid-jsx-oxc/TODO.md` — consult it before assuming a feature is missing vs. intentionally deferred (e.g. `@once`, full `universal` mode, full SSR `use:` directives).

## Workspace Layout

The repo uses **bun workspaces** (root `package.json` declares `workspaces: ["packages/*", "examples/*"]`). The Rust compiler is itself a nested cargo workspace under `packages/solid-jsx-oxc/` with member crates in `crates/*`.

Packages:

- `packages/solid-jsx-oxc` — the core Rust crate + NAPI bindings. This is where the compiler lives.
- `packages/vite-plugin-solid-oxc`, `packages/rolldown-plugin-solid-oxc`, `packages/bun-plugin-solid-oxc` — thin TS plugins that call into `solid-jsx-oxc`.
- `packages/babel-plugin-jsx-dom-expressions` — the **original** Babel plugin, kept in-tree as a reference implementation. Read this when porting/diagnosing transform behavior.
- `packages/dom-expressions` — the runtime library that the compiled code calls into (`solid-js/web` re-exports from this).

## Compiler Architecture (`packages/solid-jsx-oxc`)

Entry point is `src/lib.rs`:

1. Parses input with `oxc_parser` into an OXC AST.
2. Dispatches on `TransformOptions::generate`:
   - `Dom` → `dom::SolidTransform` (in `crates/dom`)
   - `Ssr` → `ssr::SSRTransform` (in `crates/ssr`)
   - `Universal` → currently aliased to DOM (see `TODO.md`).
3. Code-generates with `oxc_codegen`.

Each transform crate is organized the same way: `transform.rs` is the visitor entry, `element.rs` / `component.rs` handle the two JSX forms, `template.rs` builds the static template string, `ir.rs` is the intermediate representation, `output.rs` (DOM only) emits the final IIFE.

`crates/common` holds shared `TransformOptions`, the `solid_defaults()` preset, expression utilities, and the constants (e.g. delegated event list) that must stay in sync with the Babel plugin.

`crates/linter` is a separate Solid-specific linter built on the same OXC infrastructure (rules under `crates/linter/src/rules`).

There is one important `unsafe` pattern in `transform_internal` (`src/lib.rs`): a raw-pointer reborrow of `&TransformOptions` to give the visitor an independent lifetime. It is sound because `options` outlives the call; preserve this pattern (and its safety comment) if you refactor.

## Common Commands

All commands assume **bun**. Run from the repo root unless noted.

```bash
# Install everything
bun install

# Build all workspaces (compiles native module + TS plugins)
bun run build

# Run all workspace tests
bun run test

# Clean node_modules + dist
bun run clean
```

### Working on the native compiler (`packages/solid-jsx-oxc`)

```bash
cd packages/solid-jsx-oxc

# Build the .node binary (release; required before JS verification)
bun run build
bun run build:debug    # faster, unoptimized

# Rust tests (this is what `bun run test` invokes for this package)
cargo test
cargo test <name>      # filter by test name
cargo test --test transform_tests <name>   # integration tests

# Snapshot tests use `insta` — review pending snapshots:
cargo insta review

# Bench
cargo bench

# Verify the built .node loads + transforms a sample
bun run verify     # alias: bun run test:js
```

### TS plugins

Each plugin package builds with plain `tsc`:

```bash
cd packages/vite-plugin-solid-oxc   # or rolldown-plugin-solid-oxc, bun-plugin-solid-oxc
bun run build
bun run dev    # tsc --watch
```

### Examples

```bash
cd examples/tanstack-start-solid    # or test-solid-vite7, bun-solid-elysia
bun install
bun run dev
```

## Publishing

Use the root-level interactive script:

```bash
bun publish-alpha.ts                              # dry run
bun publish-alpha.ts --publish                    # interactive confirm
bun publish-alpha.ts --publish --yes              # automated
bun publish-alpha.ts --tag beta --publish --yes   # different dist-tag
bun publish-alpha.ts --only solid-jsx-oxc         # subset (with deps)
bun publish-alpha.ts --exclude babel-plugin-jsx-dom-expressions
```

Per-package release helpers also exist (e.g. `bun run release:alpha` inside `packages/solid-jsx-oxc`). The root script writes an interactive HTML report (`publish-report.html`) on completion.

## Notes for Modifications

- When changing transform behavior, cross-check against `packages/babel-plugin-jsx-dom-expressions` to confirm the intended output — that plugin is the source of truth for compatibility.
- Constants like the delegated-events list live in `crates/common/src/constants.rs`; keep them aligned with the Babel plugin.
- After modifying Rust code, plugins won't pick up changes until `bun run build` is re-run inside `packages/solid-jsx-oxc` (it produces the platform-specific `.node` file consumed by `index.js`).
- The `vite-plugin-solid-oxc` README section on `node_modules` exclusion is load-bearing for SSR frameworks (TanStack Start, SolidStart) — be careful when adjusting default `exclude` patterns.
