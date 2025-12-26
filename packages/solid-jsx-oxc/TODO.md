# solid-jsx-oxc: Unimplemented Features

This document tracks features that are not yet implemented or incomplete in the OXC-based Solid JSX transformer.

## Recently Fixed

The following issues have been fixed:

- ~~SSR Expression Container~~ - Now properly extracts and uses expressions
- ~~SSR Spread Children~~ - Now extracts spread expressions
- ~~SSR Fragment Children~~ - Now recursively processes fragment children
- ~~SSR Element with Spread~~ - Now builds proper props object and children expression
- ~~Property Bindings (`prop:`)~~ - Now transforms to direct property assignments

## High Priority

### 1. Directive Handling (`use:`)
**Status**: Partial - works in DOM, skipped in SSR

- **DOM**: `crates/dom/src/element.rs:324-349` - wrapped in generic `use()` call
- **SSR**: `crates/ssr/src/element.rs:128` - skipped entirely (directives are client-only)

### 2. SuspenseList Component
**Status**: Declared but not implemented

Listed in `BUILT_INS` but no specific transform function. Falls through to generic component handling.

## Medium Priority

### 4. `@once` Static Marker
**Location**: `crates/common/src/options.rs:50`

The `static_marker` option exists but is never used. Should skip effect wrapping for `@once` marked expressions.

### 5. Universal Mode (Isomorphic)
**Location**: `crates/common/src/options.rs:67`

`GenerateMode::Universal` variant exists but no code path uses it.

### 6. classList Object Binding
**Status**: Partially implemented, not fully tested

Complex object binding patterns like `classList={{ active: isActive() }}` may not work correctly.

### 7. Hydration Boundaries
**Status**: Partial

Hydration keys and markers are generated but comprehensive boundary marking may be incomplete.

### 8. Complex Style Objects
**Location**: `crates/dom/src/element.rs:346-388`

Only handles simple static object literals. Dynamic computed properties and nested objects are not handled.

## Low Priority

### 9. Memo Optimization
The `memo_wrapper` option exists but is unused. No `@memo` marker support.

### 10. Lazy Spread Merging
Complex conditional spreads on elements may not merge correctly.

## Known Limitations (By Design)

These differ from the Babel implementation by design:

1. **Scope Analysis**: Uses simplified `is_dynamic()` that assumes identifiers are always dynamic (safe but may over-optimize)

2. **Statement Expression Handling**: `expr_to_string` returns `"/* unsupported statement */"` for non-expression statements

3. **Complex Expression Parsing**: Expressions are parsed as strings which may lose some AST information

## Test Coverage (56 tests passing)

Features verified working:
- [x] Basic element transformation
- [x] Component transformation with props
- [x] Event handling (onClick, onInput, etc.)
- [x] Delegated events
- [x] Dynamic attributes
- [x] Static attributes
- [x] Style objects (simple cases)
- [x] innerHTML/textContent
- [x] Children (text, elements, expressions)
- [x] Fragments
- [x] SVG elements
- [x] Ref bindings
- [x] Built-in components (For, Show, Switch, Match, etc.)
- [x] Template element walking
- [x] Hydration markers
- [x] SSR expression containers
- [x] SSR spread children
- [x] SSR fragment children
- [x] SSR element with spread props
- [x] Property bindings (`prop:`)

Features needing more testing:
- [ ] classList with object binding
- [ ] Complex nested structures
- [ ] Custom elements
