/**
 * solid-jsx-oxc - OXC-based JSX compiler for SolidJS
 *
 * This is the JavaScript wrapper around the Rust NAPI bindings.
 * It provides the same interface as babel-preset-solid.
 */

// Try to load the native module
let nativeBinding = null;

// Detect platform and architecture
const platform = process.platform;
const arch = process.arch;

// Map Node.js platform/arch to binary file suffix
const platformMap = {
  'darwin-arm64': 'darwin-arm64',
  'darwin-x64': 'darwin-x64',
  'linux-x64': 'linux-x64-gnu',
  'linux-arm64': 'linux-arm64-gnu',
  'win32-x64': 'win32-x64-msvc',
  'win32-arm64': 'win32-arm64-msvc',
};

const platformKey = `${platform}-${arch}`;
const nativeTarget = platformMap[platformKey];

try {
  if (nativeTarget) {
    // Try platform-specific binary first
    nativeBinding = require(`./solid-jsx-oxc.${nativeTarget}.node`);
  } else {
    // Fallback to generic name
    nativeBinding = require('./solid-jsx-oxc.node');
  }
} catch (e) {
  // Fallback message if native module not found
  console.warn(`solid-jsx-oxc: Native module not found for ${platformKey}. Run \`npm run build\` to compile.`);
  console.warn(e.message);
}

/**
 * Default options matching babel-preset-solid
 */
const defaultOptions = {
  moduleName: 'solid-js/web',
  builtIns: [
    'For',
    'Show',
    'Switch',
    'Match',
    'Suspense',
    'SuspenseList',
    'Portal',
    'Index',
    'Dynamic',
    'ErrorBoundary'
  ],
  contextToCustomElements: true,
  wrapConditionals: true,
  generate: 'dom', // 'dom' | 'ssr' | 'universal'
  hydratable: false,
  delegateEvents: true,
  sourceMap: false,
};

/**
 * Transform JSX source code
 * @param {string} source - The source code to transform
 * @param {object} options - Transform options
 * @returns {{ code: string, map?: string }}
 */
function transform(source, options = {}) {
  if (!nativeBinding) {
    throw new Error('solid-jsx-oxc: Native module not loaded. Ensure it is built for your platform.');
  }

  const mergedOptions = { ...defaultOptions, ...options };

  // Map options to match Rust struct field names (snake_case)
  const rustOptions = {
    module_name: mergedOptions.moduleName,
    generate: mergedOptions.generate,
    hydratable: mergedOptions.hydratable,
    delegate_events: mergedOptions.delegateEvents,
    wrap_conditionals: mergedOptions.wrapConditionals,
    context_to_custom_elements: mergedOptions.contextToCustomElements,
    filename: mergedOptions.filename,
    source_map: mergedOptions.sourceMap,
  };

  return nativeBinding.transformJsx(source, rustOptions);
}

/**
 * Create a preset configuration (for compatibility with babel-preset-solid interface)
 * @param {object} context - Babel context (ignored, for compatibility)
 * @param {object} options - User options
 * @returns {object}
 */
function preset(context, options = {}) {
  const mergedOptions = { ...defaultOptions, ...options };

  return {
    // Return the options that would be passed to the transform
    options: mergedOptions,

    // The transform function
    transform: (source) => transform(source, mergedOptions),
  };
}

module.exports = {
  transform,
  preset,
  defaultOptions,
  // Also export the raw binding for advanced usage
  transformJsx: nativeBinding ? nativeBinding.transformJsx : null,
};

// Also export as default for ESM compatibility
module.exports.default = module.exports;
