//! Native element transform
//! Handles <div>, <span>, etc. -> template + effects

use oxc_allocator::CloneIn;
use oxc_ast::ast::{
    Argument, AssignmentTarget, Expression, FormalParameterKind, JSXAttribute, JSXAttributeItem,
    JSXAttributeValue, JSXElement, ObjectPropertyKind, PropertyKind, Statement,
};
use oxc_ast::AstBuilder;
use oxc_ast::NONE;
use oxc_span::{Span, SPAN};
use oxc_syntax::operator::{AssignmentOperator, BinaryOperator, UnaryOperator};
use oxc_syntax::symbol::SymbolFlags;
use oxc_traverse::TraverseCtx;

use common::{
    constants::{ALIASES, DELEGATED_EVENTS, VOID_ELEMENTS},
    expression::{escape_html, to_event_name},
    get_attr_name, is_component, is_dynamic, is_dynamic_for_spread, is_namespaced_attr,
    is_svg_element, TransformOptions,
};

use crate::component::{getter_return_expr, make_prop_key};
use crate::ir::{BlockContext, ChildTransformer, Declaration, DynamicBinding, TransformResult};
use crate::transform::TransformInfo;

fn ident_expr<'a>(ast: AstBuilder<'a>, span: Span, name: &str) -> Expression<'a> {
    let _ = span;
    ast.expression_identifier(SPAN, ast.allocator.alloc_str(name))
}

fn static_member<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    object: Expression<'a>,
    property: &str,
) -> Expression<'a> {
    let _ = span;
    let prop = ast.identifier_name(SPAN, ast.allocator.alloc_str(property));
    Expression::StaticMemberExpression(
        ast.alloc_static_member_expression(SPAN, object, prop, false),
    )
}

fn call_expr<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    callee: Expression<'a>,
    args: impl IntoIterator<Item = Expression<'a>>,
) -> Expression<'a> {
    let _ = span;
    let mut arguments = ast.vec();
    for arg in args {
        arguments.push(Argument::from(arg));
    }
    ast.expression_call(
        SPAN,
        callee,
        None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
        arguments,
        false,
    )
}

fn bool_cast_expr<'a>(ast: AstBuilder<'a>, span: Span, expr: Expression<'a>) -> Expression<'a> {
    let _ = span;
    let not_expr = ast.expression_unary(SPAN, UnaryOperator::LogicalNot, expr);
    ast.expression_unary(SPAN, UnaryOperator::LogicalNot, not_expr)
}

fn class_toggle_expr<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    elem_id: &str,
    class_name: &str,
    value: Expression<'a>,
) -> Expression<'a> {
    let elem = ident_expr(ast, span, elem_id);
    let class_list = static_member(ast, span, elem, "classList");
    let toggle = static_member(ast, span, class_list, "toggle");
    let class_name_lit = ast.expression_string_literal(SPAN, ast.allocator.alloc_str(class_name), None);
    call_expr(ast, span, toggle, [class_name_lit, value])
}

fn set_style_property_expr<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    elem_id: &str,
    prop_name: &str,
    value: Expression<'a>,
) -> Expression<'a> {
    let callee = ident_expr(ast, span, "setStyleProperty");
    let elem = ident_expr(ast, span, elem_id);
    let prop_name_lit = ast.expression_string_literal(SPAN, ast.allocator.alloc_str(prop_name), None);
    call_expr(ast, span, callee, [elem, prop_name_lit, value])
}

fn arrow_zero_params_return_expr<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    expr: Expression<'a>,
) -> Expression<'a> {
    let _ = span;
    let params = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::ArrowFormalParameters,
        ast.vec(),
        NONE,
    );
    let mut statements = ast.vec_with_capacity(1);
    statements.push(Statement::ExpressionStatement(
        ast.alloc_expression_statement(SPAN, expr),
    ));
    let body = ast.alloc_function_body(SPAN, ast.vec(), statements);
    ast.expression_arrow_function(SPAN, true, false, NONE, params, NONE, body)
}

fn expression_to_assignment_target<'a>(expr: Expression<'a>) -> Option<AssignmentTarget<'a>> {
    match expr {
        Expression::Identifier(ident) => Some(AssignmentTarget::AssignmentTargetIdentifier(ident)),
        Expression::StaticMemberExpression(m) => Some(AssignmentTarget::StaticMemberExpression(m)),
        Expression::ComputedMemberExpression(m) => {
            Some(AssignmentTarget::ComputedMemberExpression(m))
        }
        Expression::PrivateFieldExpression(m) => Some(AssignmentTarget::PrivateFieldExpression(m)),
        // Strip TS type wrappers — the inner expression is the actual assignment target.
        // Keeping the `as`/`satisfies` in assignment position produces invalid syntax
        // (e.g. `local.ref as SomeType = r$` is ambiguous/invalid).
        Expression::TSAsExpression(e) => expression_to_assignment_target(e.unbox().expression),
        Expression::TSSatisfiesExpression(e) => {
            expression_to_assignment_target(e.unbox().expression)
        }
        Expression::TSNonNullExpression(e) => {
            expression_to_assignment_target(e.unbox().expression)
        }
        Expression::TSTypeAssertion(e) => expression_to_assignment_target(e.unbox().expression),
        _ => None,
    }
}

/// Transform a native HTML/SVG element
pub fn transform_element<'a, 'b>(
    element: &JSXElement<'a>,
    tag_name: &str,
    info: &TransformInfo,
    context: &BlockContext<'a>,
    options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
    ctx: &TraverseCtx<'a, ()>,
) -> TransformResult<'a> {
    let ast = context.ast();
    let is_svg = is_svg_element(tag_name);
    let is_void = VOID_ELEMENTS.contains(tag_name);
    let is_custom_element = tag_name.contains('-');

    let mut result = TransformResult {
        span: element.span,
        tag_name: Some(tag_name.to_string()),
        is_svg,
        has_custom_element: is_custom_element,
        ..Default::default()
    };

    // Check if this element needs runtime access (dynamic attributes, refs, events)
    let needs_runtime_access = element_needs_runtime_access(element);

    // Generate element ID if needed
    if !info.skip_id && (info.top_level || needs_runtime_access) {
        let elem_id = context.generate_uid("el$");
        result.id = Some(elem_id.clone());

        // If we have a path, we need to walk to this element
        if !info.path.is_empty() {
            if let Some(root_id) = &info.root_id {
                let init = info.path.iter().fold(
                    ident_expr(ast, element.span, root_id),
                    |acc, step| static_member(ast, element.span, acc, step),
                );
                result
                    .declarations
                    .push(Declaration::single(elem_id.clone(), init));
            }
        }
    }

    // Start building template
    result.template = format!("<{}", tag_name);
    result.template_with_closing_tags = result.template.clone();

    // Transform attributes
    transform_attributes(element, &mut result, context, options, ctx);

    // Close opening tag
    result.template.push('>');
    result.template_with_closing_tags.push('>');

    // Transform children (if not void element)
    if !is_void {
        // Pass down the root ID and path for children
        // If this element has an ID, it becomes the new root for children
        // and children's paths reset to be relative to this element
        let child_info = TransformInfo {
            root_id: result.id.clone().or_else(|| info.root_id.clone()),
            path: if result.id.is_some() {
                vec![]
            } else {
                info.path.clone()
            },
            top_level: false,
            ..info.clone()
        };
        transform_children(
            element,
            &mut result,
            &child_info,
            context,
            options,
            transform_child,
            ctx,
        );

        // Close tag
        result.template.push_str(&format!("</{}>", tag_name));
        result
            .template_with_closing_tags
            .push_str(&format!("</{}>", tag_name));
    }

    // At the top of the JSX tree, emit `runHydrationEvents()` after all the
    // setup expressions if anything in this subtree had a spread (or otherwise
    // flagged a possibly-hydratable event). This mirrors babel's
    //   if (info.topLevel && config.hydratable && results.hasHydratableEvent)
    //     results.postExprs.push(callExpression(runHydrationEvents, []));
    // The runtime needs this to flush deferred event handler attachments that
    // hydration deferred while walking the SSR-emitted DOM.
    if info.top_level && context.hydratable && result.has_hydratable_event {
        context.register_helper("runHydrationEvents");
        let ast = context.ast();
        let callee = ident_expr(ast, element.span, "runHydrationEvents");
        result
            .post_exprs
            .push(call_expr(ast, element.span, callee, []));
    }

    result
}

/// Check if an element needs runtime access
fn element_needs_runtime_access(element: &JSXElement) -> bool {
    // Check attributes
    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                // Namespaced attributes like on:click or use:directive always need access
                if is_namespaced_attr(&attr.name) {
                    return true;
                }
                let key = get_attr_name(&attr.name);

                // ref and inner content setters need access
                if key == "ref" || key == "innerHTML" || key == "textContent" || key == "innerText"
                {
                    return true;
                }

                // Event handlers need access
                if key.starts_with("on") && key.len() > 2 {
                    return true;
                }

                // Any expression container needs runtime access (we may need to run setters/helpers).
                // This keeps id generation consistent with the rest of the transform.
                if matches!(&attr.value, Some(JSXAttributeValue::ExpressionContainer(_))) {
                    return true;
                }
            }
            JSXAttributeItem::SpreadAttribute(_) => {
                // Spread attributes always need runtime access
                return true;
            }
        }
    }

    // Check children for components or dynamic expressions
    // If any child is a component, we need an ID for insert() calls
    fn children_need_runtime_access<'a>(children: &[oxc_ast::ast::JSXChild<'a>]) -> bool {
        for child in children {
            match child {
                oxc_ast::ast::JSXChild::Element(child_elem) => {
                    let child_tag = common::get_tag_name(child_elem);
                    if is_component(&child_tag) {
                        return true;
                    }
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(_) => {
                    return true;
                }
                oxc_ast::ast::JSXChild::Fragment(fragment) => {
                    if children_need_runtime_access(&fragment.children) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    if children_need_runtime_access(&element.children) {
        return true;
    }

    false
}

/// Whether an attribute key can be handled by the runtime `spread()` helper
/// (i.e. folded into the merged props object) or whether it must be processed
/// through a separate per-attr expression.
///
/// Mirrors `canNativeSpread` in `babel-plugin-jsx-dom-expressions`
/// (`shared/utils.js`): the runtime `spread()` does not handle `ref` or
/// attributes whose namespace is one of `class:`, `style:`, `use:`, `prop:`,
/// `attr:`, `bool:`. Everything else (including `onClick`, `on:click`,
/// `classList`, `style`) flows through.
fn can_native_spread(key: &str) -> bool {
    if key == "ref" {
        return false;
    }
    if let Some((ns, _)) = key.split_once(':') {
        const NON_SPREAD: &[&str] = &["class", "style", "use", "prop", "attr", "bool"];
        if NON_SPREAD.contains(&ns) {
            return false;
        }
    }
    true
}

/// Transform element attributes.
///
/// When the element has any spread attribute, this delegates to
/// `process_spreads` (mirroring Babel's `processSpreads`) to fold every
/// spreadable attribute into a single `spread(elem, mergeProps(...), isSVG,
/// hasChildren)` call. Non-spreadable attrs (events with `on:`, refs,
/// directives, `prop:`/`attr:`/`class:`/`style:` namespaces) fall through to
/// the per-attribute handlers.
///
/// Without spreads, every attribute is processed individually as before.
fn transform_attributes<'a>(
    element: &JSXElement<'a>,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
    options: &TransformOptions<'a>,
    ctx: &TraverseCtx<'a, ()>,
) {
    let elem_id = result.id.clone();

    let has_spread = element
        .opening_element
        .attributes
        .iter()
        .any(|a| matches!(a, JSXAttributeItem::SpreadAttribute(_)));

    if has_spread {
        let elem_id_str = elem_id
            .as_deref()
            .expect("Spread attributes require an element id");
        let filtered = process_spreads(
            element,
            elem_id_str,
            result.is_svg,
            !element.children.is_empty(),
            result,
            context,
        );
        for &i in &filtered {
            if let JSXAttributeItem::Attribute(attr) = &element.opening_element.attributes[i] {
                transform_attribute(attr, elem_id.as_deref(), result, context, options, ctx);
            }
        }
        return;
    }

    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                transform_attribute(attr, elem_id.as_deref(), result, context, options, ctx);
            }
            JSXAttributeItem::SpreadAttribute(_) => {
                unreachable!("has_spread branch handles spreads")
            }
        }
    }
}

/// Process attributes for an element that has at least one spread.
///
/// Walks the attribute list in source order and partitions each attribute:
///
/// * **Spreads** — flush any accumulated `running_object` into `spread_args`,
///   then push the spread argument (wrapped in an arrow function if dynamic
///   per Babel's `isDynamic({checkMember: true})`).
/// * **Spreadable attrs** when either a spread has already been seen
///   (`first_spread`) OR the attribute has a dynamic expression value:
///   accumulate into `running_object`. Dynamic exprs become getter properties
///   (`get key() { return expr; }`) so they remain reactive when read by the
///   runtime; static strings/booleans become plain properties.
/// * Everything else — push the attribute's index into `filtered` so the
///   caller can run the existing per-attr transform (events, refs, directives,
///   `prop:`/`attr:`/`class:`/`style:` namespaces).
///
/// Finally builds the props expression: a single object literal if there is
/// exactly one non-dynamic spread arg; otherwise `mergeProps(...spread_args)`.
/// Emits `spread(elem, props, isSVG, hasChildren)` and sets
/// `result.has_hydratable_event = true` (we cannot know at compile time
/// whether the spread contains an event handler).
///
/// Returns the indices of the original attributes that need separate
/// per-attribute processing.
fn process_spreads<'a>(
    element: &JSXElement<'a>,
    elem_id: &str,
    is_svg: bool,
    has_children: bool,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
) -> Vec<usize> {
    let ast = context.ast();
    let mut filtered: Vec<usize> = Vec::new();
    let mut spread_args: Vec<Expression<'a>> = Vec::new();
    let mut running_object: Vec<ObjectPropertyKind<'a>> = Vec::new();
    let mut dynamic_spread = false;
    let mut first_spread = false;

    let flush_running =
        |ast: AstBuilder<'a>,
         running_object: &mut Vec<ObjectPropertyKind<'a>>,
         spread_args: &mut Vec<Expression<'a>>| {
            if running_object.is_empty() {
                return;
            }
            let mut props = ast.vec();
            for p in running_object.drain(..) {
                props.push(p);
            }
            spread_args.push(ast.expression_object(SPAN, props));
        };

    for (i, attr) in element.opening_element.attributes.iter().enumerate() {
        match attr {
            JSXAttributeItem::SpreadAttribute(spread) => {
                first_spread = true;
                flush_running(ast, &mut running_object, &mut spread_args);
                let arg_expr = context.clone_expr(&spread.argument);
                // Babel uses `isDynamic({ checkMember: true, checkCallExpressions: true })`
                // here — a plain identifier (e.g. `rest` from splitProps) is NOT
                // wrapped in an arrow, so `mergeProps` reads its getters lazily and
                // hydration-ID allocation matches Babel's order. See
                // `is_dynamic_for_spread` in `crates/common/src/check.rs`.
                let arg = if is_dynamic_for_spread(&spread.argument) {
                    dynamic_spread = true;
                    arrow_zero_params_return_expr(ast, spread.span, arg_expr)
                } else {
                    arg_expr
                };
                spread_args.push(arg);
            }
            JSXAttributeItem::Attribute(attr_inner) => {
                let key = get_attr_name(&attr_inner.name);
                // Match Babel: an attribute is "dynamic enough to fold into the
                // merged props object" when its expression value is dynamic per
                // `isDynamic({ checkMember: true, checkCallExpressions: true })`.
                // Plain identifier values are treated as static here so they
                // become regular `key: value` props (not getters).
                let dyn_expr = matches!(
                    &attr_inner.value,
                    Some(JSXAttributeValue::ExpressionContainer(c))
                        if c.expression
                            .as_expression()
                            .map(is_dynamic_for_spread)
                            .unwrap_or(false)
                );

                if (first_spread || dyn_expr) && can_native_spread(&key) {
                    let prop_key = make_prop_key(ast, attr_inner.span, &key);
                    match &attr_inner.value {
                        Some(JSXAttributeValue::ExpressionContainer(container)) => {
                            if let Some(expr) = container.expression.as_expression() {
                                if is_dynamic_for_spread(expr) {
                                    let getter = getter_return_expr(
                                        ast,
                                        attr_inner.span,
                                        context.clone_expr(expr),
                                    );
                                    running_object.push(
                                        ast.object_property_kind_object_property(
                                            SPAN,
                                            PropertyKind::Get,
                                            prop_key,
                                            getter,
                                            false,
                                            false,
                                            false,
                                        ),
                                    );
                                } else {
                                    running_object.push(
                                        ast.object_property_kind_object_property(
                                            SPAN,
                                            PropertyKind::Init,
                                            prop_key,
                                            context.clone_expr(expr),
                                            false,
                                            false,
                                            false,
                                        ),
                                    );
                                }
                            }
                        }
                        Some(JSXAttributeValue::StringLiteral(lit)) => {
                            let value = ast.expression_string_literal(
                                SPAN,
                                ast.allocator.alloc_str(&lit.value),
                                None,
                            );
                            running_object.push(ast.object_property_kind_object_property(
                                SPAN,
                                PropertyKind::Init,
                                prop_key,
                                value,
                                false,
                                false,
                                false,
                            ));
                        }
                        None => {
                            // Boolean attribute (no value). Babel emits "" for
                            // attribute-style keys (Properties.has(key) would
                            // give a boolean true, but for the common case of
                            // HTML boolean attrs the spread runtime treats ""
                            // as truthy). Keep it as the empty-string literal
                            // for parity with the most common Babel output.
                            let value =
                                ast.expression_string_literal(SPAN, ast.allocator.alloc_str(""), None);
                            running_object.push(ast.object_property_kind_object_property(
                                SPAN,
                                PropertyKind::Init,
                                prop_key,
                                value,
                                false,
                                false,
                                false,
                            ));
                        }
                        _ => {}
                    }
                } else {
                    filtered.push(i);
                }
            }
        }
    }

    flush_running(ast, &mut running_object, &mut spread_args);

    // Build the props expression handed to spread().
    let props_expr = if spread_args.len() == 1 && !dynamic_spread {
        spread_args.into_iter().next().unwrap()
    } else {
        context.register_helper("mergeProps");
        let callee = ident_expr(ast, element.span, "mergeProps");
        call_expr(ast, element.span, callee, spread_args)
    };

    context.register_helper("spread");
    let spread_callee = ident_expr(ast, element.span, "spread");
    let elem = ident_expr(ast, element.span, elem_id);
    let is_svg_lit = ast.expression_boolean_literal(SPAN, is_svg);
    let has_children_lit = ast.expression_boolean_literal(SPAN, has_children);
    let spread_call = call_expr(
        ast,
        element.span,
        spread_callee,
        [elem, props_expr, is_svg_lit, has_children_lit],
    );
    result.exprs.push(spread_call);

    // Babel: spread is opaque to the compiler — assume it could carry an event
    // handler so the top-level element emits runHydrationEvents().
    result.has_hydratable_event = true;

    filtered
}

/// Transform a single attribute
fn transform_attribute<'a>(
    attr: &JSXAttribute<'a>,
    elem_id: Option<&str>,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
    options: &TransformOptions<'a>,
    ctx: &TraverseCtx<'a, ()>,
) {
    let key = get_attr_name(&attr.name);

    // Handle different attribute types
    if key == "ref" {
        let elem_id = elem_id.expect("ref requires an element id");
        transform_ref(attr, elem_id, result, context, ctx);
        return;
    }

    if key.starts_with("on") {
        let elem_id = elem_id.expect("event handlers require an element id");
        transform_event(attr, &key, elem_id, result, context, options);
        return;
    }

    if key.starts_with("use:") {
        let elem_id = elem_id.expect("directives require an element id");
        transform_directive(attr, &key, elem_id, result, context);
        return;
    }

    // Handle prop: prefix - direct DOM property assignment
    if key.starts_with("prop:") {
        let elem_id = elem_id.expect("prop: requires an element id");
        transform_prop(attr, &key, elem_id, result, context);
        return;
    }

    // Handle attr: prefix - force attribute mode
    if key.starts_with("attr:") {
        let elem_id = elem_id.expect("attr: requires an element id");
        transform_attr(attr, &key, elem_id, result, context);
        return;
    }

    // Handle class: prefix - classList.toggle() behavior
    if key.starts_with("class:") {
        let elem_id = elem_id.expect("class: requires an element id");
        transform_class_namespace(attr, &key, elem_id, result, context);
        return;
    }

    // Handle style: prefix - setStyleProperty() behavior
    if key.starts_with("style:") {
        let elem_id = elem_id.expect("style: requires an element id");
        transform_style_namespace(attr, &key, elem_id, result, context);
        return;
    }

    // Handle style attribute specially
    if key == "style" {
        transform_style(attr, elem_id, result, context);
        return;
    }

    // Handle innerHTML/textContent
    if key == "innerHTML" || key == "textContent" {
        let elem_id = elem_id.expect("inner content requires an element id");
        transform_inner_content(attr, &key, elem_id, result, context);
        return;
    }

    // Regular attribute
    match &attr.value {
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            // Static string attribute - inline in template
            let attr_key = ALIASES.get(key.as_str()).copied().unwrap_or(key.as_str());
            let escaped = escape_html(&lit.value, true);
            result
                .template
                .push_str(&format!(" {}=\"{}\"", attr_key, escaped));
        }
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            // Dynamic attribute - needs effect
            if let Some(expr) = container.expression.as_expression() {
                if is_dynamic(expr) {
                    // Dynamic - wrap in effect
                    let elem_id = elem_id.expect("dynamic attributes require an element id");
                    result.dynamics.push(DynamicBinding {
                        elem: elem_id.to_string(),
                        key: key.clone(),
                        value: context.clone_expr(expr),
                        is_svg: result.is_svg,
                        is_ce: result.has_custom_element,
                        tag_name: result.tag_name.clone().unwrap_or_default(),
                    });
                } else {
                    // Static expression - we need to evaluate it at build time
                    // For now, treat as dynamic to be safe
                    let elem_id = elem_id.expect("expression attributes require an element id");
                    result.dynamics.push(DynamicBinding {
                        elem: elem_id.to_string(),
                        key: key.clone(),
                        value: context.clone_expr(expr),
                        is_svg: result.is_svg,
                        is_ce: result.has_custom_element,
                        tag_name: result.tag_name.clone().unwrap_or_default(),
                    });
                }
            }
        }
        None => {
            // Boolean attribute (e.g., disabled)
            result.template.push_str(&format!(" {}", key));
        }
        _ => {}
    }
}

/// Transform ref attribute
fn transform_ref<'a>(
    attr: &JSXAttribute<'a>,
    elem_id: &str,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
    ctx: &TraverseCtx<'a, ()>,
) {
    let ast = context.ast();
    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            let ref_expr = context.clone_expr(expr);
            let elem = ident_expr(ast, attr.span, elem_id);
            // Check if it's a function expression (arrow function or function expression)
            if matches!(
                expr,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            ) {
                // It's an inline callback: ref={el => myRef = el}
                // Just invoke it with the element
                result
                    .exprs
                    .push(call_expr(ast, attr.span, ref_expr, [elem]));
            } else {
                // It's a variable reference: ref={myRef}
                // Could be a signal setter or plain variable - check at runtime
                if is_writable_ref_target(expr, ctx) {
                    // Non-const variable: generate typeof check with assignment fallback
                    let typeof_ref = ast.expression_unary(
                        SPAN,
                        UnaryOperator::Typeof,
                        ref_expr.clone_in(ast.allocator),
                    );
                    let function_str = ast.expression_string_literal(
                        SPAN,
                        ast.allocator.alloc_str("function"),
                        None,
                    );
                    let test = ast.expression_binary(
                        SPAN,
                        typeof_ref,
                        BinaryOperator::StrictEquality,
                        function_str,
                    );

                    let call = call_expr(
                        ast,
                        attr.span,
                        ref_expr.clone_in(ast.allocator),
                        [elem.clone_in(ast.allocator)],
                    );

                    let assign =
                        expression_to_assignment_target(ref_expr.clone_in(ast.allocator))
                            .map(|target| {
                                ast.expression_assignment(
                                    SPAN,
                                    AssignmentOperator::Assign,
                                    target,
                                    elem.clone_in(ast.allocator),
                                )
                            })
                            .unwrap_or_else(|| ast.expression_identifier(SPAN, "undefined"));

                    result
                        .exprs
                        .push(ast.expression_conditional(SPAN, test, call, assign));
                } else {
                    // Const/import binding: must be a function (e.g., signal setter), just call it
                    result
                        .exprs
                        .push(call_expr(ast, attr.span, ref_expr, [elem]));
                }
            }
        }
    }
}

pub(crate) fn is_writable_ref_target<'a>(expr: &Expression<'a>, ctx: &TraverseCtx<'a, ()>) -> bool {
    let Some(ident) = peel_identifier_reference(expr) else {
        return true;
    };

    let Some(reference_id) = ident.reference_id.get() else {
        return true;
    };

    let reference = ctx.scoping.scoping().get_reference(reference_id);
    let Some(symbol_id) = reference.symbol_id() else {
        return true;
    };

    let flags = ctx.scoping.scoping().symbol_flags(symbol_id);
    !(flags.is_const_variable() || flags.contains(SymbolFlags::Import) || flags.contains(SymbolFlags::TypeImport))
}

fn peel_identifier_reference<'a, 'b>(
    expr: &'b Expression<'a>,
) -> Option<&'b oxc_ast::ast::IdentifierReference<'a>> {
    match expr {
        Expression::Identifier(ident) => Some(ident),
        Expression::TSAsExpression(e) => peel_identifier_reference(&e.expression),
        Expression::TSSatisfiesExpression(e) => peel_identifier_reference(&e.expression),
        Expression::TSNonNullExpression(e) => peel_identifier_reference(&e.expression),
        Expression::TSTypeAssertion(e) => peel_identifier_reference(&e.expression),
        _ => None,
    }
}

/// Transform event handler
fn transform_event<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
    options: &TransformOptions<'a>,
) {
    let ast = context.ast();
    // Check for capture mode (onClickCapture -> click with capture=true)
    let is_capture = key.ends_with("Capture");
    let base_key = if is_capture {
        &key[..key.len() - 7] // Remove "Capture" suffix
    } else {
        key
    };

    let event_name = to_event_name(base_key);

    // Get the handler expression
    let handler = attr
        .value
        .as_ref()
        .and_then(|v| match v {
            JSXAttributeValue::ExpressionContainer(container) => {
                container.expression.as_expression()
            }
            _ => None,
        })
        .map(|e| context.clone_expr(e))
        .unwrap_or_else(|| ast.expression_identifier(SPAN, "undefined"));

    // on: prefix forces non-delegation (direct addEventListener)
    let force_no_delegate = key.starts_with("on:");

    // Capture events cannot be delegated
    // Check if this event should be delegated
    let should_delegate = !force_no_delegate
        && !is_capture
        && options.delegate_events
        && (DELEGATED_EVENTS.contains(event_name.as_str())
            || options.delegated_events.contains(&event_name.as_str()));

    if should_delegate {
        context.register_delegate(&event_name);
        let elem = ident_expr(ast, attr.span, elem_id);
        let prop = format!("$${}", event_name);
        let member = static_member(ast, attr.span, elem, &prop);
        let Some(target) = expression_to_assignment_target(member) else {
            return;
        };
        result.exprs.push(ast.expression_assignment(
            SPAN,
            AssignmentOperator::Assign,
            target,
            handler,
        ));
    } else {
        context.register_helper("addEventListener");
        let callee = ident_expr(ast, attr.span, "addEventListener");
        let elem = ident_expr(ast, attr.span, elem_id);
        let event = ast.expression_string_literal(SPAN, ast.allocator.alloc_str(&event_name), None);
        let capture = ast.expression_boolean_literal(SPAN, is_capture);
        result.exprs.push(call_expr(
            ast,
            attr.span,
            callee,
            [elem, event, handler, capture],
        ));
    }
}

/// Transform use: directive
fn transform_directive<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
) {
    let ast = context.ast();
    context.register_helper("use");
    let directive_name = &key[4..]; // Strip "use:"

    let value = attr
        .value
        .as_ref()
        .and_then(|v| match v {
            JSXAttributeValue::ExpressionContainer(container) => {
                container.expression.as_expression()
            }
            _ => None,
        })
        .map(|e| arrow_zero_params_return_expr(ast, attr.span, context.clone_expr(e)))
        .unwrap_or_else(|| ast.expression_identifier(SPAN, "undefined"));

    let callee = ident_expr(ast, attr.span, "use");
    result.exprs.push(call_expr(
        ast,
        attr.span,
        callee,
        [
            ident_expr(ast, attr.span, directive_name),
            ident_expr(ast, attr.span, elem_id),
            value,
        ],
    ));
}

/// Transform prop: prefix (direct DOM property assignment)
fn transform_prop<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
) {
    let ast = context.ast();
    let prop_name = &key[5..]; // Strip "prop:"

    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            let elem = ident_expr(ast, attr.span, elem_id);
            let member = static_member(ast, attr.span, elem, prop_name);
            let Some(target) = expression_to_assignment_target(member) else {
                return;
            };
            let assign = ast.expression_assignment(
                SPAN,
                AssignmentOperator::Assign,
                target,
                context.clone_expr(expr),
            );

            if is_dynamic(expr) {
                context.register_helper("effect");
                let effect = ident_expr(ast, attr.span, "effect");
                let arrow = arrow_zero_params_return_expr(ast, attr.span, assign);
                result
                    .exprs
                    .push(call_expr(ast, attr.span, effect, [arrow]));
            } else {
                result.exprs.push(assign);
            }
        }
    }
}

/// Transform attr: prefix (force attribute mode via setAttribute)
fn transform_attr<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
) {
    let ast = context.ast();
    let attr_name = &key[5..]; // Strip "attr:"

    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            context.register_helper("effect");
            context.register_helper("setAttribute");
            let elem = ident_expr(ast, attr.span, elem_id);
            let set_attr = static_member(ast, attr.span, elem, "setAttribute");
            let name =
                ast.expression_string_literal(SPAN, ast.allocator.alloc_str(attr_name), None);
            let call = call_expr(ast, attr.span, set_attr, [name, context.clone_expr(expr)]);
            let arrow = arrow_zero_params_return_expr(ast, attr.span, call);
            let effect = ident_expr(ast, attr.span, "effect");
            result
                .exprs
                .push(call_expr(ast, attr.span, effect, [arrow]));
        }
    } else if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
        // Static value - inline in template
        let escaped = escape_html(&lit.value, true);
        result
            .template
            .push_str(&format!(" {}=\"{}\"", attr_name, escaped));
    }
}

/// Transform class: prefix (maps to classList.toggle)
fn transform_class_namespace<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
) {
    let ast = context.ast();
    let class_name = &key[6..]; // Strip "class:"

    match &attr.value {
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            if let Some(expr) = container.expression.as_expression() {
                let toggle_expr = class_toggle_expr(
                    ast,
                    attr.span,
                    elem_id,
                    class_name,
                    bool_cast_expr(ast, attr.span, context.clone_expr(expr)),
                );

                if is_dynamic(expr) {
                    context.register_helper("effect");
                    let effect = ident_expr(ast, attr.span, "effect");
                    let arrow = arrow_zero_params_return_expr(ast, attr.span, toggle_expr);
                    result
                        .exprs
                        .push(call_expr(ast, attr.span, effect, [arrow]));
                } else {
                    result.exprs.push(toggle_expr);
                }
            }
        }
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            let truthy = ast.expression_boolean_literal(SPAN, !lit.value.is_empty());
            result
                .exprs
                .push(class_toggle_expr(ast, attr.span, elem_id, class_name, truthy));
        }
        None => {
            let truthy = ast.expression_boolean_literal(SPAN, true);
            result
                .exprs
                .push(class_toggle_expr(ast, attr.span, elem_id, class_name, truthy));
        }
        _ => {}
    }
}

/// Transform style: prefix (maps to setStyleProperty)
fn transform_style_namespace<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
) {
    let ast = context.ast();
    let prop_name = &key[6..]; // Strip "style:"
    context.register_helper("setStyleProperty");

    match &attr.value {
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            if let Some(expr) = container.expression.as_expression() {
                let set_prop = set_style_property_expr(
                    ast,
                    attr.span,
                    elem_id,
                    prop_name,
                    context.clone_expr(expr),
                );

                if is_dynamic(expr) {
                    context.register_helper("effect");
                    let effect = ident_expr(ast, attr.span, "effect");
                    let arrow = arrow_zero_params_return_expr(ast, attr.span, set_prop);
                    result
                        .exprs
                        .push(call_expr(ast, attr.span, effect, [arrow]));
                } else {
                    result.exprs.push(set_prop);
                }
            }
        }
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            let value =
                ast.expression_string_literal(SPAN, ast.allocator.alloc_str(&lit.value), None);
            result.exprs.push(set_style_property_expr(
                ast, attr.span, elem_id, prop_name, value,
            ));
        }
        _ => {}
    }
}

/// Transform style attribute
fn transform_style<'a>(
    attr: &JSXAttribute<'a>,
    elem_id: Option<&str>,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
) {
    let ast = context.ast();
    match &attr.value {
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            // Static style string - inline in template
            result
                .template
                .push_str(&format!(" style=\"{}\"", escape_html(&lit.value, true)));
        }
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            if let Some(expr) = container.expression.as_expression() {
                // Check if it's an object expression (static object)
                if let oxc_ast::ast::Expression::ObjectExpression(obj) = expr {
                    // Try to convert to static style string
                    if let Some(style_str) = object_to_style_string(obj) {
                        result
                            .template
                            .push_str(&format!(" style=\"{}\"", style_str));
                        return;
                    }
                }

                // Dynamic style - use style helper
                let elem_id = elem_id.expect("style helper requires an element id");
                context.register_helper("style");
                let elem = ident_expr(ast, attr.span, elem_id);
                let style = ident_expr(ast, attr.span, "style");
                let call = call_expr(ast, attr.span, style, [elem, context.clone_expr(expr)]);
                if is_dynamic(expr) {
                    context.register_helper("effect");
                    let arrow = arrow_zero_params_return_expr(ast, attr.span, call);
                    let effect = ident_expr(ast, attr.span, "effect");
                    result
                        .exprs
                        .push(call_expr(ast, attr.span, effect, [arrow]));
                } else {
                    result.exprs.push(call);
                }
            }
        }
        None => {}
        _ => {}
    }
}

/// Try to convert a static object expression to a style string
fn object_to_style_string(obj: &oxc_ast::ast::ObjectExpression) -> Option<String> {
    let mut styles = Vec::new();

    for prop in &obj.properties {
        if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) = prop {
            // Get key
            let key = match &prop.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => {
                    // Convert camelCase to kebab-case
                    camel_to_kebab(&id.name)
                }
                oxc_ast::ast::PropertyKey::StringLiteral(lit) => lit.value.to_string(),
                _ => return None, // Dynamic key, can't inline
            };

            // Get value - must be a static literal
            let value = match &prop.value {
                oxc_ast::ast::Expression::StringLiteral(lit) => lit.value.to_string(),
                oxc_ast::ast::Expression::NumericLiteral(num) => {
                    // Add px for numeric values (except certain properties)
                    let num_str = num.value.to_string();
                    if needs_px_suffix(&key) && num.value != 0.0 {
                        format!("{}px", num_str)
                    } else {
                        num_str
                    }
                }
                _ => return None, // Dynamic value, can't inline
            };

            styles.push(format!("{}: {}", key, value));
        } else {
            return None; // Spread or method, can't inline
        }
    }

    Some(styles.join("; "))
}

/// Convert camelCase to kebab-case
fn camel_to_kebab(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('-');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// Check if a CSS property needs px suffix for numeric values
fn needs_px_suffix(prop: &str) -> bool {
    // Properties that don't need px suffix
    let unitless = [
        "animation-iteration-count",
        "border-image-outset",
        "border-image-slice",
        "border-image-width",
        "box-flex",
        "box-flex-group",
        "box-ordinal-group",
        "column-count",
        "columns",
        "flex",
        "flex-grow",
        "flex-positive",
        "flex-shrink",
        "flex-negative",
        "flex-order",
        "grid-row",
        "grid-row-end",
        "grid-row-span",
        "grid-row-start",
        "grid-column",
        "grid-column-end",
        "grid-column-span",
        "grid-column-start",
        "font-weight",
        "line-clamp",
        "line-height",
        "opacity",
        "order",
        "orphans",
        "tab-size",
        "widows",
        "z-index",
        "zoom",
        "fill-opacity",
        "flood-opacity",
        "stop-opacity",
        "stroke-dasharray",
        "stroke-dashoffset",
        "stroke-miterlimit",
        "stroke-opacity",
        "stroke-width",
    ];
    !unitless.contains(&prop)
}

/// Transform innerHTML/textContent
fn transform_inner_content<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult<'a>,
    context: &BlockContext<'a>,
) {
    let ast = context.ast();
    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            let elem = ident_expr(ast, attr.span, elem_id);
            let member = static_member(ast, attr.span, elem, key);
            let Some(target) = expression_to_assignment_target(member) else {
                return;
            };
            let assign = ast.expression_assignment(
                SPAN,
                AssignmentOperator::Assign,
                target,
                context.clone_expr(expr),
            );

            if is_dynamic(expr) {
                context.register_helper("effect");
                let arrow = arrow_zero_params_return_expr(ast, attr.span, assign);
                let effect = ident_expr(ast, attr.span, "effect");
                result
                    .exprs
                    .push(call_expr(ast, attr.span, effect, [arrow]));
            } else {
                result.exprs.push(assign);
            }
        }
    } else if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
        // Static string - but we still need to set it at runtime for innerHTML
        if key == "innerHTML" {
            let elem = ident_expr(ast, attr.span, elem_id);
            let member = static_member(ast, attr.span, elem, "innerHTML");
            let Some(target) = expression_to_assignment_target(member) else {
                return;
            };
            let value = ast.expression_string_literal(
                SPAN,
                ast.allocator.alloc_str(&escape_html(&lit.value, false)),
                None,
            );
            result.exprs.push(ast.expression_assignment(
                SPAN,
                AssignmentOperator::Assign,
                target,
                value,
            ));
        } else {
            // textContent can be inlined in template
            // But the element should have no children then
        }
    }
}

/// Transform element children
fn transform_children<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult<'a>,
    info: &TransformInfo,
    context: &BlockContext<'a>,
    options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
    ctx: &TraverseCtx<'a, ()>,
) {
    /// Walker state for the children traversal. Mirrors Babel's `tempPath`
    /// pattern: instead of always walking from the parent's `firstChild`,
    /// we re-anchor the walker after every hydratable `<!/>` marker. This
    /// is required because in hydratable mode the SSR DOM contains
    /// variable-length content between each `<!--$-->/<!--/-->` marker pair.
    /// Counting `nextSibling` hops from the parent's `firstChild` therefore
    /// produces the wrong target — the walk must instead chain off the
    /// most-recently-declared close marker.
    ///
    /// In non-hydratable mode the walker is never re-anchored, so this
    /// degenerates to the previous "root.<path_prefix>.firstChild.nextSibling^N"
    /// behavior — equivalent to the old `child_path(&info.path, node_index)`.
    struct WalkerState {
        /// Position counter for the next child to be emitted.
        node_index: usize,
        /// Whether the most recent text child still wants to merge with
        /// adjacent text (mirrors Babel's `lastWasText`).
        last_was_text: bool,
        /// Variable name to chain accesses from. Initially the parent
        /// element's id (or the topmost template root when the parent
        /// element is itself static and undeclared); switches to the
        /// close-marker id after every hydratable marker pair.
        root: String,
        /// Path segments from `root` to the parent element. Empty when
        /// `root` is the parent itself; otherwise the inherited
        /// `info.path` from the static-only ancestor chain. After
        /// re-anchoring this is cleared (the close marker has no prefix
        /// to its position).
        path_prefix: Vec<String>,
        /// Position of the anchor (the close marker) in the children
        /// sequence. Unused while `is_parent` is true; after re-anchoring
        /// it is the `node_index` of the close marker at the moment it
        /// was declared, so subsequent accesses use
        /// `nextSibling^(N - anchor)`.
        anchor: usize,
        /// `true` when `root` (after `path_prefix`) refers to the parent
        /// element — the first hop into the children list is `firstChild`.
        /// `false` when `root` is a sibling marker — the first hop is
        /// `nextSibling` (the close marker has no children to descend
        /// into; the next position is its sibling).
        is_parent: bool,
    }

    impl WalkerState {
        /// Initial state for a children traversal. When the current
        /// element has its own declared id, walker chains directly off
        /// it (`path_prefix` empty). When it doesn't (e.g. static-only
        /// nested element with no runtime needs), walker chains off the
        /// inherited `info.root_id` with `info.path` as prefix —
        /// matching the old `child_path(&info.path, …)` behavior.
        fn from_info(result_id: Option<&str>, info: &TransformInfo) -> Self {
            let (root, prefix) = match (result_id, info.root_id.as_deref()) {
                (Some(id), _) => (id.to_string(), Vec::new()),
                (None, Some(rid)) => (rid.to_string(), info.path.clone()),
                (None, None) => (String::new(), Vec::new()),
            };
            Self {
                node_index: 0,
                last_was_text: false,
                root,
                path_prefix: prefix,
                anchor: 0,
                is_parent: true,
            }
        }

        /// Build the path segments from `root` to `target_index`. For a
        /// parent walker this is `path_prefix ++ ["firstChild",
        /// "nextSibling"; N]`, for a sibling-anchored walker it is
        /// `["nextSibling"; N - anchor]` (the `nextSibling`-only chain
        /// because the close marker is itself a sibling, not the parent
        /// of subsequent nodes).
        fn path_to(&self, target_index: usize) -> Vec<String> {
            if self.is_parent {
                let mut path =
                    Vec::with_capacity(self.path_prefix.len() + target_index + 1);
                path.extend(self.path_prefix.iter().cloned());
                path.push("firstChild".to_string());
                for _ in 0..target_index {
                    path.push("nextSibling".to_string());
                }
                path
            } else {
                let steps = target_index.saturating_sub(self.anchor);
                let mut path = Vec::with_capacity(steps);
                for _ in 0..steps {
                    path.push("nextSibling".to_string());
                }
                path
            }
        }

        /// Build the AST `Expression` for the access at `target_index`,
        /// relative to the current walker root.
        fn accessor<'a>(
            &self,
            ast: AstBuilder<'a>,
            span: Span,
            target_index: usize,
        ) -> Expression<'a> {
            let mut expr = ident_expr(ast, span, &self.root);
            for step in self.path_to(target_index) {
                expr = static_member(ast, span, expr, &step);
            }
            expr
        }

        /// Re-anchor the walker to a freshly-declared close marker, so
        /// subsequent positional accesses chain off the marker rather
        /// than from the parent. Only used in hydratable mode.
        fn reanchor(&mut self, marker_id: String, anchor_index: usize) {
            self.root = marker_id;
            self.path_prefix = Vec::new();
            self.anchor = anchor_index;
            self.is_parent = false;
        }
    }

    /// Check if children list is a single dynamic expression (no markers needed).
    ///
    /// A "single dynamic child" means the parent host element will receive
    /// exactly one dynamically-inserted value at runtime and nothing else,
    /// so the compiled `insert(parent, value)` call doesn't need a marker
    /// argument and the template doesn't need a `<!>` placeholder. This
    /// must mirror Babel's `checkLength` + insertion logic in
    /// `babel-plugin-jsx-dom-expressions/src/dom/element.js`: a component
    /// child counts as a single dynamic expression, not as static
    /// "other content" — the whole point is that the component renders
    /// dynamically into the parent.
    ///
    /// Native (lowercase) elements remain static template content and
    /// disqualify the single-dynamic path.
    fn is_single_dynamic_child(children: &[oxc_ast::ast::JSXChild<'_>]) -> bool {
        let mut expr_count = 0;
        let mut other_content = false;

        for child in children {
            match child {
                oxc_ast::ast::JSXChild::Text(text) => {
                    let content = common::expression::trim_whitespace(&text.value);
                    if !content.is_empty() {
                        other_content = true;
                    }
                }
                oxc_ast::ast::JSXChild::Element(child_elem) => {
                    let tag = common::get_tag_name(child_elem);
                    if is_component(&tag) {
                        // Component children are dynamic insertions, not
                        // static template content. They count toward the
                        // single-dynamic-child rule.
                        expr_count += 1;
                    } else {
                        other_content = true;
                    }
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                    if container.expression.as_expression().is_some() {
                        expr_count += 1;
                    }
                }
                oxc_ast::ast::JSXChild::Fragment(fragment) => {
                    // Recurse into fragments
                    if !is_single_dynamic_child(&fragment.children) {
                        other_content = true;
                    } else {
                        expr_count += 1;
                    }
                }
                _ => {}
            }
        }

        expr_count == 1 && !other_content
    }

    fn transform_children_list<'a, 'b>(
        children: &[oxc_ast::ast::JSXChild<'a>],
        result: &mut TransformResult<'a>,
        info: &TransformInfo,
        context: &BlockContext<'a>,
        options: &TransformOptions<'a>,
        transform_child: ChildTransformer<'a, 'b>,
        ctx: &TraverseCtx<'a, ()>,
        walker: &mut WalkerState,
        single_dynamic: bool,
    ) {
        let ast = context.ast();
        for child in children {
            match child {
                oxc_ast::ast::JSXChild::Text(text) => {
                    let content = common::expression::trim_whitespace(&text.value);
                    if !content.is_empty() {
                        let escaped = escape_html(&content, false);
                        result.template.push_str(&escaped);
                        result.template_with_closing_tags.push_str(&escaped);
                        if !walker.last_was_text {
                            walker.node_index += 1;
                            walker.last_was_text = true;
                        }
                    }
                }
                oxc_ast::ast::JSXChild::Element(child_elem) => {
                    let child_tag = common::get_tag_name(child_elem);

                    if is_component(&child_tag) {
                        walker.last_was_text = false;
                        if let (Some(parent_id), Some(child_result)) =
                            (result.id.as_deref(), transform_child(child))
                        {
                            if child_result.exprs.is_empty() {
                                continue;
                            }

                            context.register_helper("insert");

                            // Single dynamic child: no marker needed
                            if single_dynamic {
                                let callee = ident_expr(ast, child_elem.span, "insert");
                                let parent = ident_expr(ast, child_elem.span, parent_id);
                                let child_expr = child_result.exprs[0].clone_in(ast.allocator);
                                result.exprs.push(call_expr(
                                    ast,
                                    child_elem.span,
                                    callee,
                                    [parent, child_expr],
                                ));
                            } else if context.hydratable {
                                // Hydratable mode: emit a `<!$><!/>` marker
                                // pair that mirrors the SSR output's
                                // `<!--$-->...<!--/-->` boundary, and use
                                // `getNextMarker` to walk the SSR-emitted
                                // DOM between them. The 4-arg `insert(parent,
                                // value, marker, current)` form gives the
                                // runtime the existing hydrated content
                                // array so it can replace it in place
                                // instead of trying to create new nodes
                                // (which the dev runtime catches as
                                // "Failed attempt to create new DOM
                                // elements during hydration").
                                //
                                // After emitting the close marker we
                                // re-anchor the walker to it so the next
                                // positional access chains off the close
                                // marker. The SSR DOM has variable-length
                                // content between each marker pair, so a
                                // count-from-firstChild walk lands on the
                                // wrong target after the first pair.
                                result.template.push_str("<!$><!/>");
                                result.template_with_closing_tags.push_str("<!$><!/>");

                                let open_id = context.generate_uid("el$");
                                let open_init = walker.accessor(
                                    ast,
                                    child_elem.span,
                                    walker.node_index,
                                );
                                result
                                    .declarations
                                    .push(Declaration::single(open_id.clone(), open_init));

                                context.register_helper("getNextMarker");
                                let marker_id = context.generate_uid("el$");
                                let content_id = context.generate_uid("co$");
                                let next_sibling = static_member(
                                    ast,
                                    child_elem.span,
                                    ident_expr(ast, child_elem.span, &open_id),
                                    "nextSibling",
                                );
                                let getter = ident_expr(
                                    ast,
                                    child_elem.span,
                                    "getNextMarker",
                                );
                                let getter_call = call_expr(
                                    ast,
                                    child_elem.span,
                                    getter,
                                    [next_sibling],
                                );
                                result.declarations.push(Declaration::array_pair(
                                    marker_id.clone(),
                                    content_id.clone(),
                                    getter_call,
                                ));

                                let callee = ident_expr(ast, child_elem.span, "insert");
                                let parent = ident_expr(ast, child_elem.span, parent_id);
                                let child_expr = child_result.exprs[0].clone_in(ast.allocator);
                                let marker = ident_expr(ast, child_elem.span, &marker_id);
                                let content = ident_expr(ast, child_elem.span, &content_id);
                                result.exprs.push(call_expr(
                                    ast,
                                    child_elem.span,
                                    callee,
                                    [parent, child_expr, marker, content],
                                ));

                                let close_index = walker.node_index + 1;
                                walker.node_index += 2;
                                walker.reanchor(marker_id, close_index);
                            } else {
                                result.template.push_str("<!>");
                                result.template_with_closing_tags.push_str("<!>");

                                let marker_id = context.generate_uid("el$");
                                let marker_init = walker.accessor(
                                    ast,
                                    child_elem.span,
                                    walker.node_index,
                                );
                                result.declarations.push(Declaration::single(
                                    marker_id.clone(),
                                    marker_init,
                                ));

                                let callee = ident_expr(ast, child_elem.span, "insert");
                                let parent = ident_expr(ast, child_elem.span, parent_id);
                                let child_expr = child_result.exprs[0].clone_in(ast.allocator);
                                let marker = ident_expr(ast, child_elem.span, &marker_id);
                                result.exprs.push(call_expr(
                                    ast,
                                    child_elem.span,
                                    callee,
                                    [parent, child_expr, marker],
                                ));

                                walker.node_index += 1;
                            }
                        }
                        continue;
                    }

                    walker.last_was_text = false;
                    // Build the child element's path from the walker's
                    // current root (the parent element initially, or the
                    // most recent close marker after a hydratable pair).
                    // We also override `root_id` so the child's id init
                    // expression is folded from the walker root rather
                    // than the original template root.
                    let child_path_segments = walker.path_to(walker.node_index);
                    let child_info = TransformInfo {
                        top_level: false,
                        path: child_path_segments,
                        root_id: Some(walker.root.clone()),
                        ..info.clone()
                    };

                    let child_result = transform_element(
                        child_elem,
                        &child_tag,
                        &child_info,
                        context,
                        options,
                        transform_child,
                        ctx,
                    );

                    result.template.push_str(&child_result.template);
                    if !child_result.template_with_closing_tags.is_empty() {
                        result
                            .template_with_closing_tags
                            .push_str(&child_result.template_with_closing_tags);
                    } else {
                        result
                            .template_with_closing_tags
                            .push_str(&child_result.template);
                    }
                    result.declarations.extend(child_result.declarations);
                    result.exprs.extend(child_result.exprs);
                    result.dynamics.extend(child_result.dynamics);
                    result.has_custom_element |= child_result.has_custom_element;
                    result.has_hydratable_event |= child_result.has_hydratable_event;

                    walker.node_index += 1;
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                    if let (Some(parent_id), Some(expr)) =
                        (result.id.as_deref(), container.expression.as_expression())
                    {
                        walker.last_was_text = false;
                        context.register_helper("insert");

                        let insert_value = if is_dynamic(expr) {
                            arrow_zero_params_return_expr(
                                ast,
                                container.span,
                                context.clone_expr(expr),
                            )
                        } else {
                            context.clone_expr(expr)
                        };

                        // Single dynamic child: no marker needed
                        if single_dynamic {
                            let callee = ident_expr(ast, container.span, "insert");
                            let parent = ident_expr(ast, container.span, parent_id);
                            result.exprs.push(call_expr(
                                ast,
                                container.span,
                                callee,
                                [parent, insert_value],
                            ));
                        } else if context.hydratable {
                            // Hydratable mode: emit `<!$><!/>` marker pair
                            // and a `[marker, content] = getNextMarker(...)`
                            // destructure so the runtime can hydrate this
                            // dynamic insertion against the SSR-emitted
                            // `<!--$-->...<!--/-->` content. See the
                            // matching component-child branch above for
                            // the rationale and the walker re-anchor.
                            result.template.push_str("<!$><!/>");
                            result.template_with_closing_tags.push_str("<!$><!/>");

                            let open_id = context.generate_uid("el$");
                            let open_init =
                                walker.accessor(ast, container.span, walker.node_index);
                            result
                                .declarations
                                .push(Declaration::single(open_id.clone(), open_init));

                            context.register_helper("getNextMarker");
                            let marker_id = context.generate_uid("el$");
                            let content_id = context.generate_uid("co$");
                            let next_sibling = static_member(
                                ast,
                                container.span,
                                ident_expr(ast, container.span, &open_id),
                                "nextSibling",
                            );
                            let getter = ident_expr(ast, container.span, "getNextMarker");
                            let getter_call =
                                call_expr(ast, container.span, getter, [next_sibling]);
                            result.declarations.push(Declaration::array_pair(
                                marker_id.clone(),
                                content_id.clone(),
                                getter_call,
                            ));

                            let callee = ident_expr(ast, container.span, "insert");
                            let parent = ident_expr(ast, container.span, parent_id);
                            let marker = ident_expr(ast, container.span, &marker_id);
                            let content = ident_expr(ast, container.span, &content_id);
                            result.exprs.push(call_expr(
                                ast,
                                container.span,
                                callee,
                                [parent, insert_value, marker, content],
                            ));

                            let close_index = walker.node_index + 1;
                            walker.node_index += 2;
                            walker.reanchor(marker_id, close_index);
                        } else {
                            result.template.push_str("<!>");
                            result.template_with_closing_tags.push_str("<!>");

                            let marker_id = context.generate_uid("el$");
                            let marker_init =
                                walker.accessor(ast, container.span, walker.node_index);
                            result.declarations.push(Declaration::single(
                                marker_id.clone(),
                                marker_init,
                            ));

                            let callee = ident_expr(ast, container.span, "insert");
                            let parent = ident_expr(ast, container.span, parent_id);
                            let marker = ident_expr(ast, container.span, &marker_id);
                            result.exprs.push(call_expr(
                                ast,
                                container.span,
                                callee,
                                [parent, insert_value, marker],
                            ));

                            walker.node_index += 1;
                        }
                    }
                }
                oxc_ast::ast::JSXChild::Fragment(fragment) => {
                    transform_children_list(
                        &fragment.children,
                        result,
                        info,
                        context,
                        options,
                        transform_child,
                        ctx,
                        walker,
                        single_dynamic,
                    );
                }
                _ => {}
            }
        }
    }

    let mut walker = WalkerState::from_info(result.id.as_deref(), info);
    let single_dynamic = is_single_dynamic_child(&element.children);
    transform_children_list(
        &element.children,
        result,
        info,
        context,
        options,
        transform_child,
        ctx,
        &mut walker,
        single_dynamic,
    );
}
