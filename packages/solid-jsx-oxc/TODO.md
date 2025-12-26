# solid-jsx-oxc: Unimplemented Features

This document tracks features that are not yet implemented or incomplete in the OXC-based Solid JSX transformer.

## Critical Issues

These issues prevent certain patterns from working correctly:

### 1. SSR Expression Container - Placeholder Implementation
**Location**: `crates/ssr/src/transform.rs:128-140`

Expression containers in SSR output `/* expr */` instead of the actual expression.

```rust
// Current (broken):
result.push_dynamic("/* expr */".to_string(), false, false);

// Should be:
let expr_str = expr_to_string(expr);
result.push_dynamic(format!("escape({})", expr_str), false, false);
```

### 2. SSR Spread Children - Placeholder Implementation
**Location**: `crates/ssr/src/transform.rs:71-77`

Spread children output `/* spread */` instead of the spread expression.

### 3. SSR Fragment Children - TODO
**Location**: `crates/ssr/src/element.rs:290-294`

Fragment children inside elements are not processed. The loop exists but does nothing.

```rust
oxc_ast::ast::JSXChild::Fragment(fragment) => {
    for _frag_child in &fragment.children {
        // TODO: Handle fragment children similarly
    }
}
```

### 4. SSR Element with Spread - Placeholder Props/Children
**Location**: `crates/ssr/src/element.rs:68-93`

Elements with spread attributes output placeholder props and children:

```rust
format!(
    "ssrElement(\"{}\", /* props */, /* children */, {})",
    tag_name,
    hydratable
)
```

## High Priority

### 5. Directive Handling (`use:`)
**Status**: Partial - works in DOM, skipped in SSR

- **DOM**: `crates/dom/src/element.rs:320-344` - wrapped in generic `use()` call
- **SSR**: `crates/ssr/src/element.rs:128` - skipped entirely

### 6. Property Bindings (`prop:`)
**Status**: Recognized but skipped

The `prop:` prefix is detected but not transformed into property assignments.

### 7. SuspenseList Component
**Status**: Declared but not implemented

Listed in `BUILT_INS` but no specific transform function. Falls through to generic component handling.

## Medium Priority

### 8. `@once` Static Marker
**Location**: `crates/common/src/options.rs:50`

The `static_marker` option exists but is never used. Should skip effect wrapping for `@once` marked expressions.

### 9. Universal Mode (Isomorphic)
**Location**: `crates/common/src/options.rs:67`

`GenerateMode::Universal` variant exists but no code path uses it.

### 10. classList Object Binding
**Status**: Partially implemented, not fully tested

Complex object binding patterns like `classList={{ active: isActive() }}` may not work correctly.

### 11. Hydration Boundaries
**Status**: Partial

Hydration keys and markers are generated but comprehensive boundary marking may be incomplete.

### 12. Complex Style Objects
**Location**: `crates/dom/src/element.rs:346-388`

Only handles simple static object literals. Dynamic computed properties and nested objects are not handled.

## Low Priority

### 13. Memo Optimization
The `memo_wrapper` option exists but is unused. No `@memo` marker support.

### 14. Lazy Spread Merging
Complex conditional spreads on elements may not merge correctly.

## Known Limitations (By Design)

These differ from the Babel implementation by design:

1. **Scope Analysis**: Uses simplified `is_dynamic()` that assumes identifiers are always dynamic (safe but may over-optimize)

2. **Statement Expression Handling**: `expr_to_string` returns `"/* unsupported statement */"` for non-expression statements

3. **Complex Expression Parsing**: Expressions are parsed as strings which may lose some AST information

## Test Coverage

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

Features needing more testing:
- [ ] classList with object binding
- [ ] Complex nested structures
- [ ] All SSR features
- [ ] Spread props on elements
- [ ] Custom elements
