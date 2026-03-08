//! Solid lint rules
//!
//! Rules ported from eslint-plugin-solid

pub mod components_return_once;
pub mod event_handlers;
pub mod imports;
pub mod jsx_no_duplicate_props;
pub mod jsx_no_script_url;
pub mod jsx_no_undef;
pub mod jsx_uses_vars;
pub mod no_array_handlers;
pub mod no_destructure;
pub mod no_innerhtml;
pub mod no_proxy_apis;
pub mod no_react_deps;
pub mod no_react_specific_props;
pub mod no_unknown_namespaces;
pub mod prefer_classlist;
pub mod prefer_for;
pub mod prefer_show;
pub mod reactivity;
pub mod self_closing_comp;
pub mod style_prop;
pub mod validate_jsx_nesting;

// Re-export rule structs
pub use components_return_once::ComponentsReturnOnce;
pub use event_handlers::EventHandlers;
pub use imports::Imports;
pub use jsx_no_duplicate_props::JsxNoDuplicateProps;
pub use jsx_no_script_url::JsxNoScriptUrl;
pub use jsx_no_undef::JsxNoUndef;
pub use jsx_uses_vars::JsxUsesVars;
pub use no_array_handlers::NoArrayHandlers;
pub use no_destructure::NoDestructure;
pub use no_innerhtml::NoInnerhtml;
pub use no_proxy_apis::NoProxyApis;
pub use no_react_deps::NoReactDeps;
pub use no_react_specific_props::NoReactSpecificProps;
pub use no_unknown_namespaces::NoUnknownNamespaces;
pub use prefer_classlist::PreferClasslist;
pub use prefer_for::PreferFor;
pub use prefer_show::PreferShow;
pub use reactivity::Reactivity;
pub use self_closing_comp::SelfClosingComp;
pub use style_prop::StyleProp;
pub use validate_jsx_nesting::ValidateJsxNesting;
