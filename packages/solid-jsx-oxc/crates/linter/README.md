# solid-linter

Solid-specific lint rules for oxlint, ported from [eslint-plugin-solid](https://github.com/solidjs-community/eslint-plugin-solid).

## Overview

This crate provides lint rules for Solid.js that can be:

1. **Used standalone** with oxc AST for custom tooling
2. **Integrated with oxlint** as a plugin (future)
3. **Enhanced with type-aware analysis** via tsgolint integration (future)

## Rules

### Correctness Rules

| Rule | Description |
|------|-------------|
| `jsx-no-duplicate-props` | Disallow passing the same prop twice in JSX |
| `jsx-no-script-url` | Disallow `javascript:` URLs in JSX attributes |
| `no-react-specific-props` | Disallow React-specific `className`/`htmlFor` props |
| `no-innerhtml` | Disallow unsafe `innerHTML` usage; detect `dangerouslySetInnerHTML` |
| `no-unknown-namespaces` | Enforce Solid-specific namespace prefixes (on:, use:, prop:, etc.) |
| `prefer-for` | Prefer `<For />` component over `Array.map()` for rendering lists |
| `style-prop` | Enforce kebab-case CSS properties and object-style syntax |

### Style Rules

| Rule | Description |
|------|-------------|
| `self-closing-comp` | Enforce self-closing for components without children |
| `prefer-show` | Prefer `<Show />` component for conditional rendering |
| `prefer-classlist` | Prefer `classList` prop over classnames helpers (clsx, cn, classnames) |

## Usage

```rust
use solid_linter::rules::{JsxNoDuplicateProps, NoReactSpecificProps, NoInnerhtml};
use solid_linter::RuleMeta;

// Create a rule instance
let rule = JsxNoDuplicateProps::new();

// Check a JSX element (requires oxc AST)
// let diagnostics = rule.check(&opening_element, &children);
```

## Type-Aware Rules (Future)

For more sophisticated analysis like the `reactivity` rule, we plan to integrate with oxlint's type-aware infrastructure (tsgolint). This would enable:

- **Signal detection** - Know if a variable is actually a signal type
- **Props type tracking** - Detect props destructuring accurately  
- **Reactivity analysis** - Track reactive values through function boundaries
- **Store access detection** - Detect Store/SetStoreFunction types

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

### Phase 2: Scope-aware rules
- [ ] `jsx-no-undef`
- [ ] `jsx-uses-vars`
- [ ] `components-return-once`

### Phase 3: Type-aware rules (requires tsgolint integration)
- [ ] `reactivity`
- [ ] `no-destructure`
- [ ] `event-handlers`

## License

MIT
