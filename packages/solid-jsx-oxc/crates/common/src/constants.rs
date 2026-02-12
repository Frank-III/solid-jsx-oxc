//! Constants ported from dom-expressions/src/constants.js
//! These define which attributes are properties, delegated events, etc.

use phf::{phf_set, Set};

/// Properties that should be set as DOM properties rather than attributes
pub static PROPERTIES: Set<&'static str> = phf_set! {
    "value",
    "checked",
    "selected",
    "muted",
};

/// Child properties that affect children
pub static CHILD_PROPERTIES: Set<&'static str> = phf_set! {
    "innerHTML",
    "textContent",
    "innerText",
    "children",
};

/// Attribute aliases (JSX name -> DOM name)
pub static ALIASES: phf::Map<&'static str, &'static str> = phf::phf_map! {
};

/// Events that can be delegated (bubbling events)
pub static DELEGATED_EVENTS: Set<&'static str> = phf_set! {
    "beforeinput",
    "click",
    "dblclick",
    "contextmenu",
    "focusin",
    "focusout",
    "input",
    "keydown",
    "keyup",
    "mousedown",
    "mousemove",
    "mouseout",
    "mouseover",
    "mouseup",
    "pointerdown",
    "pointermove",
    "pointerout",
    "pointerover",
    "pointerup",
    "touchend",
    "touchmove",
    "touchstart",
};

/// SVG elements
pub static SVG_ELEMENTS: Set<&'static str> = phf_set! {
    "svg",
    "animate",
    "animateMotion",
    "animateTransform",
    "circle",
    "clipPath",
    "defs",
    "desc",
    "ellipse",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
    "filter",
    "foreignObject",
    "g",
    "image",
    "line",
    "linearGradient",
    "marker",
    "mask",
    "metadata",
    "mpath",
    "path",
    "pattern",
    "polygon",
    "polyline",
    "radialGradient",
    "rect",
    "set",
    "stop",
    "switch",
    "symbol",
    "text",
    "textPath",
    "title",
    "tspan",
    "use",
    "view",
};

/// Void elements (self-closing)
pub static VOID_ELEMENTS: Set<&'static str> = phf_set! {
    "area",
    "base",
    "br",
    "col",
    "embed",
    "hr",
    "img",
    "input",
    "link",
    "meta",
    "param",
    "source",
    "track",
    "wbr",
};

/// Solid's built-in control flow components
pub static BUILT_INS: Set<&'static str> = phf_set! {
    "For",
    "Show",
    "Switch",
    "Match",
    "Suspense",
    "SuspenseList",
    "Portal",
    "Index",
    "Dynamic",
    "ErrorBoundary",
};
