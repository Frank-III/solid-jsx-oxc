# solid-jsx-oxc: Unimplemented Features

This document tracks features that are not yet implemented or incomplete in the OXC-based Solid JSX transformer.

## Recently Fixed

The following issues have been fixed:

- ~~SSR Expression Container~~ - Now properly extracts and uses expressions
- ~~SSR Spread Children~~ - Now extracts spread expressions
- ~~SSR Fragment Children~~ - Now recursively processes fragment children
- ~~SSR Element with Spread~~ - Now builds proper props object and children expression
- ~~Property Bindings (`prop:`)~~ - Now transforms to direct property assignments
- ~~SuspenseList Component~~ - No special-case transform required; handled by normal component transform (same as Babel)
- ~~SSR component nested inside an HTML element loses props/children~~ - `<nav><Show><A href="/x">hi</A></Show></nav>` used to compile the inner `<A>` to `createComponent(A, {})` because element.rs's nested-component child transformer fell back to a stub for any deeper component. Fixed by introducing a recursive `transform_jsx_child` helper.
- ~~Hydratable DOM mode emitted `cloneNode` instead of `getNextElement`~~ - The DOM transform was ignoring `hydratable: true` for the template-instantiation step: every compiled component emitted `const _el$1 = _tmpl$1.cloneNode(true)`, which builds a fresh DOM tree from scratch. The dev runtime catches this with `"Failed attempt to create new DOM elements during hydration. Check that the libraries you are using support hydration."` (thrown from `template()` in `solid-js/web/dist/dev.js`); the prod runtime silently re-renders the entire page client-side, completely defeating SSR. Fixed by threading `hydratable` through `BlockContext` and emitting `const _el$1 = getNextElement(_tmpl$1)` (matching `babel-plugin-jsx-dom-expressions/src/dom/template.js:registerTemplate`) so the runtime walks the SSR-emitted DOM via `sharedConfig.registry` instead of cloning a new tree.
- ~~SSR hydration markers caused full client re-render on first paint~~ - Three independent bugs combined: (1) marker token was `<!--#-->` but the runtime's `getNextMarker` matches `<!--$-->`; (2) `push_dynamic` defaulted `needs_marker = !is_attr` which leaked markers *inside* opening tags around `ssrHydrationKey()` / `ssrStyle()` / `ssrClassList()` / `ssrAttribute()` interpolations, producing malformed HTML the browser silently mis-parsed; (3) markers were emitted around every dynamic child regardless of sibling count, but Babel's rule is `markers = hydratable && multi` (only when the parent has >1 meaningful children). Symptom: visible "HTML flashing" because hydration silently fell back to a full client-side render. Fixed by switching the marker token, making `push_dynamic` default to `needs_marker = false`, switching the four in-tag helper sites to `push_dynamic_with_marker(..., false)`, and computing `multi = count_meaningful_children() > 1` in `process_jsx_children` so only multi-children parents wrap dynamic children with `<!--$-->`/`<!--/-->`.
- ~~SSR spread element output was redundantly wrapped in `` ssr`${ssrElement(...)}` ``~~ - Babel's `createTemplate` short-circuits with `if (!result.template) return result.exprs[0]` (`src/ssr/template.js`) when the JSX node has produced no static template content. OXC's `to_ssr_expression` always built a tagged template literal, so a spread element's bare `ssrElement(...)` call ended up wrapped as `` ssr`${ssrElement(...)}` ``. The HTML output was identical, but the *expression shape* differed: a `CallExpression(ssrElement, …)` vs a `TaggedTemplate(ssr, …)`. That shape feeds into how parents and hydration traversal dispatch on results, and in a hydratable build it desynced `sharedConfig.getNextContextId()` allocation between SSR and DOM, surfacing as `Hydration Mismatch. Unable to find DOM nodes for hydration key …` for `<a></a>` (the visible structure matched, only the key-allocation order had drifted). Fixed by adding `SSRResult::needs_ssr_wrapper()` and a Babel-parity short-circuit in `to_ssr_expression`: when there is exactly one `skip_escape` template value, no `is_attr`, no `needs_hydration_marker`, and all static parts are empty, return the expression directly. The `ssr` helper is also only imported when the wrapping is actually emitted.
- ~~Hydratable DOM spread elements emitted divergent shape from Babel, breaking hydration~~ - For elements with any spread (e.g. `@solidjs/router`'s `<A>`: `<a {...rest} href={x()} link aria-current={ac}/>`), the OXC DOM transform was: (1) baking static-string and boolean attrs into the template (`template('<a link></a>')`), (2) emitting the spread as `spread(_el$, rest, false, false)` and then *separate* `effect()` calls for each dynamic attr, and (3) never calling `runHydrationEvents()`. All three diverge from Babel and break SSR hydration: the template's extra `link` attr produces a `data-hk` mismatch against the SSR-emitted `<a>` (which has `link=""` set at runtime by the spread on both sides, not in the template) → `Hydration Mismatch. Unable to find DOM nodes for hydration key…`. Fixed by porting Babel's `processSpreads` to `crates/dom/src/element.rs`: when an element has any spread, every spreadable attribute (everything except `ref`, `class:`/`style:`/`use:`/`prop:`/`attr:`/`bool:` namespaces) folds into a single `spread(elem, mergeProps(spread1, …, { get dynKey() { … }, staticKey: "…", boolKey: "" }), isSVG, hasChildren)` call. Dynamic exprs become getter properties (preserves reactivity); boolean attrs become `""`. `result.has_hydratable_event` propagates up the tree, and the top-level element appends `runHydrationEvents()` to `post_exprs` so deferred event handler attachments flush after hydration walks the SSR DOM. Threaded `BlockContext.hydratable` to gate the `runHydrationEvents()` emission.
- ~~SSR `transform_element_with_spread` did not match Babel semantics~~ - Source like `<a {...rest} href={x} link/>` (body of `@solidjs/router`'s `<A>`) used to emit `ssrElement("a", {...rest, href: x, link: true}, null, true)`. The eager `{...rest}` evaluated all of `rest`'s getters at object-literal time (losing reactivity), and the explicit `null` for children prevented the runtime from reading `rest.children`. Now emits `ssrElement("a", mergeProps(rest, { link: true, get href() {…} }), undefined, true)` matching Babel: spreads stay as separate `mergeProps` arguments (preserving laziness), dynamic inline attrs use getters, static-string attrs are no longer pre-escaped (the runtime escapes), and self-closing-with-spread passes `undefined` so `props.children` flows through.
- ~~SSR wrapped spread-element children in hydration markers, breaking parent template walks~~ - For JSX like `<label><input {...rest}/>{children}</label>`, the SSR output was `ssr\`<label>` + `<!--$-->${ssrElement("input", …)}<!--/-->` + `<!--$-->${escape(children)}<!--/-->` + `</label>\``. The client-side compiler treats the input as a static template chunk inside the label (`<label><input ...>...`), so post-hydration `label.firstChild` resolved to the `<!--$-->` comment instead of the input. Refs and event handlers wired to a comment node and silently no-op'd — surfaced in Gothab as a `@gothab/ui` Checkbox whose `onChange` fired on the DOM but never reached the consumer. Babel's reference plugin (`src/ssr/element.js:509,514`) uses `!child.spreadElement` to skip marker emission for the spread case. Fixed in `crates/ssr/src/element.rs::process_jsx_children` by changing the wrap condition from `wrap && (is_comp || child_result.has_spread)` to `wrap && is_comp` — components still take markers (their `createComponent`/`escape` calls need them for the hydration walk), but `ssrElement(...)` output for spread-element children does not. Pinned by `ssr_regression_spread_child_is_not_wrapped_in_hydration_markers` in `tests/transform_tests.rs`.

## High Priority

### 1. Directive Handling (`use:`)
**Status**: Partial - works in DOM, skipped in SSR

- **DOM**: `crates/dom/src/element.rs:324-349` - wrapped in generic `use()` call
- **SSR**: `crates/ssr/src/element.rs:128` - skipped entirely (directives are client-only)

## Medium Priority

### 3. Source Maps
**Status**: Partial - option exists (`source_map`) and codegen can emit a map, but mapping accuracy + tooling integration still need work.

- Validate mappings for DOM/SSR transforms (inserted helpers, templates, wrapped expressions)
- Provide bundler/plugin guidance for map chaining (Vite/Rollup/esbuild)
- Add tests that assert map correctness (golden fixtures)

### 6. classList Object Binding
**Status**: Partially implemented, not fully tested

Complex object binding patterns like `classList={{ active: isActive() }}` may not work correctly.

### 7. Hydration Boundaries
**Status**: Partial

Hydration keys and markers are generated but comprehensive boundary marking may be incomplete.

### 8. Complex Style Objects
**Location**: `crates/dom/src/element.rs:346-388`

Only handles simple static object literals. Dynamic computed properties and nested objects are not handled.

## Deferred / Not Planned (for now)

### `@once` Static Marker
**Status**: Not implemented (deferred)

This requires mapping `Program.comments` onto expressions/attributes by `Span` (OXC doesn’t attach `leadingComments` to nodes like Babel).

### Universal Mode (Isomorphic)
**Status**: Not implemented (deferred)

Implementing a true Solid “universal” output would require a separate transform pipeline (different runtime helpers + semantics).

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

## Test Coverage (65 integration tests passing)

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
