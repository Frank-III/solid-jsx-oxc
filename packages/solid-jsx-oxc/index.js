/**
 * solid-jsx-oxc - OXC-based JSX compiler for SolidJS
 *
 * This is the JavaScript wrapper around the Rust NAPI bindings.
 * It provides the same interface as babel-preset-solid.
 */

const { platform, arch } = process;

// Platform/arch to NAPI target mapping
const platformArchMap = {
  'darwin-arm64': 'darwin-arm64',
  'darwin-x64': 'darwin-x64',
  'linux-arm64': 'linux-arm64-gnu',
  'linux-x64': 'linux-x64-gnu',
  'win32-x64': 'win32-x64-msvc',
  'win32-arm64': 'win32-arm64-msvc',
};

const target = platformArchMap[`${platform}-${arch}`] || `${platform}-${arch}`;

// Try to load the native module
let nativeBinding = null;

try {
  // Try platform-specific file first
  nativeBinding = require(`./solid-jsx-oxc.${target}.node`);
} catch (e1) {
  try {
    // Fallback to generic name
    nativeBinding = require('./solid-jsx-oxc.node');
  } catch (e2) {
    // Fallback message if native module not found
    console.warn('solid-jsx-oxc: Native module not found. Run `npm run build` to compile.');
  }
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
  return nativeBinding.transformJsx(source, mergedOptions);
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
};

// Also export as default for ESM compatibility
module.exports.default = module.exports;
