# solid-linter

Solid-specific lint rules for oxlint, ported from [eslint-plugin-solid](https://github.com/solidjs-community/eslint-plugin-solid).

## Overview

This crate provides lint rules for Solid.js that can be:

1. **Used standalone** with oxc AST for custom tooling
2. **Integrated with oxlint** as a plugin (future)
3. **Enhanced with type-aware analysis** via tsgolint (Go) for Phase 3 rules

## Rules

### Correctness Rules

| Rule | Description |
|------|-------------|
| `jsx-no-duplicate-props` | Disallow passing the same prop twice in JSX |
| `jsx-no-script-url` | Disallow `javascript:` URLs in JSX attributes |
| `jsx-no-undef` | Disallow references to undefined variables in JSX (with auto-import for Solid components) |
| `jsx-uses-vars` | Mark variables used in JSX as "used" to prevent false positives from no-unused-vars |
| `no-react-specific-props` | Disallow React-specific `className`/`htmlFor` props |
| `no-innerhtml` | Disallow unsafe `innerHTML` usage; detect `dangerouslySetInnerHTML` |
| `no-unknown-namespaces` | Enforce Solid-specific namespace prefixes (on:, use:, prop:, etc.) |
| `prefer-for` | Prefer `<For />` component over `Array.map()` for rendering lists |
| `style-prop` | Enforce kebab-case CSS properties and object-style syntax |
| `components-return-once` | Disallow early returns in components (Solid components run once) |
| `no-destructure` | Disallow destructuring props (breaks Solid's reactivity) - heuristic-based |
| `reactivity` | Enforce reactive expressions are accessed properly - heuristic-based |
| `event-handlers` | Enforce consistent event handler naming and prevent immediate invocation |

### Style Rules

| Rule | Description |
|------|-------------|
| `self-closing-comp` | Enforce self-closing for components without children |
| `prefer-show` | Prefer `<Show />` component for conditional rendering |
| `prefer-classlist` | Prefer `classList` prop over classnames helpers (clsx, cn, classnames) |

## Usage

### Basic Usage (AST-only)

```rust
use solid_linter::{lint, RulesConfig};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

let source = r#"<div className="foo" />"#;
let allocator = Allocator::default();
let source_type = SourceType::jsx();
let ret = Parser::new(&allocator, source, source_type).parse();

let result = lint(source, &ret.program);
// result.diagnostics contains all lint errors/warnings
```

### Semantic-Aware Usage (Phase 2)

For rules that require scope resolution:

```rust
use solid_linter::{lint_with_semantic, SemanticRulesConfig};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

let source = r#"
import { createSignal, Show } from 'solid-js';

function App() {
    const [count, setCount] = createSignal(0);
    return <Show when={count() > 0}>{count()}</Show>;
}
"#;

let allocator = Allocator::default();
let source_type = SourceType::jsx();
let ret = Parser::new(&allocator, source, source_type).parse();

let semantic_ret = SemanticBuilder::new()
    .with_excess_capacity(0.0)
    .build(&ret.program);

let result = lint_with_semantic(
    &semantic_ret.semantic,
    source,
    source_type,
    &ret.program
);

// result.diagnostics - lint errors/warnings
// result.used_symbols - symbols marked as used in JSX
// result.component_symbols - symbols identified as components
```

## Roadmap

### Phase 1: Non-type-aware rules ✅ Complete
- [x] `jsx-no-duplicate-props`
- [x] `jsx-no-script-url`
- [x] `no-react-specific-props`
- [x] `self-closing-comp`
- [x] `no-innerhtml`
- [x] `no-unknown-namespaces`
- [x] `prefer-for`
- [x] `prefer-show`
- [x] `style-prop`
- [x] `prefer-classlist`

### Phase 2: Scope-aware rules ✅ Complete
- [x] `jsx-no-undef` - Uses oxc_semantic for scope resolution
- [x] `jsx-uses-vars` - Marks JSX identifiers as used via SymbolId
- [x] `components-return-once` - Detects components via PascalCase + JSX return heuristics

### Phase 3: Type-aware rules (requires tsgolint)
- [ ] `reactivity` - **Requires tsgolint** (Go) for TypeScript type information
- [ ] `no-destructure` - **Requires tsgolint** (Go) for TypeScript type information  
- [ ] `event-handlers` - Currently uses intrinsic tag detection (heuristic)

> **Note on Type-Aware Linting:**
>
> **Oxlint Architecture:** Oxlint (Rust) does NOT have a native TypeScript type checker. 
> Type-aware rules are handled by tsgolint (Go), which uses typescript-go for actual type information.
> The Rust side only has `oxc_semantic` (scopes, symbols, bindings) but NOT TypeScript types.
>
> **For True Type-Aware Solid Rules:** Add rules to [tsgolint](https://github.com/oxc-project/tsgolint) (Go):
> 1. Fork/contribute to tsgolint
> 2. Add Solid-specific rules that query TypeScript types (e.g., `Accessor<T>`, `Signal<T>`)
> 3. Use typescript-go's type checker to detect signal getters, stores, etc.
> 4. Rules would appear as `solid/reactivity`, `solid/no-destructure` in oxlint config

## Architecture

The linter provides two main interfaces:

1. **`LintRunner`** - Fast AST-only visitor for Phase 1 rules
2. **`SemanticLintRunner`** - Semantic-aware visitor for Phase 2 rules (scope resolution, symbol tracking)

Both use the `RuleMeta` trait for rule metadata:

```rust
pub trait RuleMeta {
    const NAME: &'static str;
    const CATEGORY: RuleCategory;
}
```

## License

MIT
