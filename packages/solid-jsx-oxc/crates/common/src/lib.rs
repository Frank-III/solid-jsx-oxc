pub mod check;
pub mod constants;
pub mod expression;
pub mod options;
pub mod xhtml;

pub use check::{
    find_prop, find_prop_value, get_attr_name, get_attr_value, get_tag_name, is_built_in,
    is_component, is_dynamic, is_dynamic_for_spread, is_namespaced_attr, is_svg_element,
};
pub use constants::*;
pub use expression::{
    escape_html, expr_to_string, get_children_callback, stmt_to_string, to_event_name,
    trim_whitespace,
};
pub use options::*;
