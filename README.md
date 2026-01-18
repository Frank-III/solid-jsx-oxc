# solid-jsx-oxc

A high-performance JSX compiler for SolidJS built with [OXC](https://oxc.rs/) and Rust.

## Features

- **Fast** - Built on OXC's Rust-based parser and transformer
- **Comprehensive** - Covers most SolidJS JSX patterns for DOM + SSR builds
- **Native** - NAPI-RS bindings for seamless Node.js integration
- **Compatible** - Aims to be a drop-in replacement for `babel-plugin-jsx-dom-expressions` in common setups (see `packages/solid-jsx-oxc/TODO.md` for gaps/deferrals)

## Installation

```bash
npm install solid-jsx-oxc
# or
bun add solid-jsx-oxc
# or
pnpm add solid-jsx-oxc
```

## Usage

### With Vite

```bash
npm install vite-plugin-solid-oxc
```

```js
// vite.config.js
import { defineConfig } from 'vite';
import solidOxc from 'vite-plugin-solid-oxc';

export default defineConfig({
  plugins: [solidOxc()],
});
```

#### SolidStart / TanStack Start / deps that ship JSX

By default, `vite-plugin-solid-oxc` excludes `node_modules` for performance. Some Solid ecosystem packages ship `.jsx/.tsx` in `node_modules` (common in SSR frameworks and routers), so those dependencies must be transformed too.

If you see JSX parse errors coming from a dependency, allowlist the packages that ship JSX/TSX:

```js
// vite.config.js
import { defineConfig } from 'vite';
import solidOxc from 'vite-plugin-solid-oxc';

export default defineConfig({
  plugins: [
    solidOxc({
      // Keep most of node_modules excluded, but compile these packages.
      exclude: [
        /node_modules\/(?!(?:@solidjs\/[^/]*|@tanstack\/solid-start|@tanstack\/solid-router[^/]*|lucide-solid)\/)/,
      ],
      // For SSR frameworks that hydrate on the client, you likely also want:
      // hydratable: true,
    }),
  ],
});
```

To compile *all* dependencies (closer to `vite-plugin-solid` behavior), use `exclude: []`.

### With Rolldown

```bash
npm install rolldown-plugin-solid-oxc
```

```js
// rolldown.config.js
import solidOxc from 'rolldown-plugin-solid-oxc';

export default {
  plugins: [solidOxc()],
};
```

### Direct API Usage

```js
import { transform } from 'solid-jsx-oxc';

const result = transform(code, {
  generate: 'dom', // 'dom' | 'ssr' | 'universal' (currently aliases 'dom')
  filename: 'input.jsx',
  moduleName: 'solid-js/web',
  builtIns: ['For', 'Show', 'Switch', 'Match', 'Suspense', 'SuspenseList', 'ErrorBoundary', 'Portal', 'Index', 'Dynamic'],
  delegateEvents: true,
  wrapConditionals: true,
  contextToCustomElements: true,
  hydratable: false,
  sourceMap: false,
});

console.log(result.code);
```

## Supported Features

| Feature | Status |
|---------|--------|
| Basic elements & attributes | ✅ |
| Dynamic attributes | ✅ |
| Event delegation (`onClick`) | ✅ |
| Non-delegated events (`on:click`) | ✅ |
| Capture events (`onClickCapture`) | ✅ |
| `prop:` prefix | ✅ |
| `attr:` prefix | ✅ |
| `classList` object | ⚠️ (complex cases need more coverage) |
| `style` object | ✅ |
| Refs (variable & callback) | ✅ |
| Spread props | ✅ |
| Built-in components (`For`, `Show`, etc.) | ✅ |
| Directives (`use:`) | ✅ (DOM) / ⚠️ (SSR skipped) |
| SVG elements | ✅ |
| Fragments | ✅ |
| SSR mode | ✅ |
| `@once` static marker | ❌ |
| Universal mode (`generate: "universal"`) | ⚠️ (currently aliases DOM) |

## Packages

| Package | Description |
|---------|-------------|
| [solid-jsx-oxc](./packages/solid-jsx-oxc) | Core OXC-based JSX compiler |
| [vite-plugin-solid-oxc](./packages/vite-plugin-solid-oxc) | Vite plugin |
| [rolldown-plugin-solid-oxc](./packages/rolldown-plugin-solid-oxc) | Rolldown plugin |
| [babel-plugin-jsx-dom-expressions](./packages/babel-plugin-jsx-dom-expressions) | Original Babel plugin (for reference) |
| [dom-expressions](./packages/dom-expressions) | Runtime library |

## Examples

| Example | Description |
|---------|-------------|
| [test-solid-vite7](./examples/test-solid-vite7) | Basic Vite + SolidJS app |
| [tanstack-start-solid](./examples/tanstack-start-solid) | TanStack Start with SSR |

```bash
# Run an example
cd examples/tanstack-start-solid
bun install
bun run dev
```

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (or Node.js 18+)

### Building

```bash
# Install dependencies
bun install

# Build the native module
cd packages/solid-jsx-oxc
bun run build

# Run tests
bun run test
```

### Testing

```bash
# Run Rust tests
cd packages/solid-jsx-oxc
cargo test

# Run all tests
bun run test
```

### Publishing

The repository includes an interactive publish script that uses Bun's Terminal API for real-time output:

```bash
# Dry run (default)
bun publish-alpha.ts

# Publish with interactive confirmation
bun publish-alpha.ts --publish

# Publish automatically (no confirmation)
bun publish-alpha.ts --publish --yes

# With 2FA
bun publish-alpha.ts --publish --yes --otp 123456

# Exclude packages
bun publish-alpha.ts --exclude babel-plugin-jsx-dom-expressions --exclude dom-expressions

# Custom tag
bun publish-alpha.ts --tag beta --publish --yes

# Options
#   --tag <name>              Dist-tag (default: alpha)
#   --only <pkg>              Only publish package and deps (repeatable)
#   --exclude <pkg>           Exclude packages (repeatable)
#   --publish                 Actually publish (default: dry-run)
#   --yes                     Skip confirmation
#   --tolerate-republish      Allow republishing same version
#   --allow-dirty             Allow uncommitted changes
#   --otp <code>              2FA code
#   --list                    Show publish order and exit
```

The script generates an interactive HTML report with clickable npm package links at the end.

## License

MIT

## Related Projects

- [SolidJS](https://github.com/solidjs/solid) - A declarative JavaScript library for building user interfaces
- [OXC](https://oxc.rs/) - The JavaScript Oxidation Compiler
- [dom-expressions](https://github.com/ryansolid/dom-expressions) - Original DOM Expressions runtime
