/**
 * solid-jsx-oxc - OXC-based JSX compiler for SolidJS
 *
 * ESM entry point - provides the same interface as babel-preset-solid.
 */

import { createRequire } from 'node:module';
import { platform, arch } from 'node:process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

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

// Try to load the native module
let nativeBinding = null;

try {
  if (nativeTarget) {
    // Try platform-specific binary first
    nativeBinding = require(join(__dirname, `solid-jsx-oxc.${nativeTarget}.node`));
  } else {
    // Fallback to generic name
    nativeBinding = require(join(__dirname, 'solid-jsx-oxc.node'));
  }
} catch (e) {
  // Fallback message if native module not found
  console.warn(`solid-jsx-oxc: Native module not found for ${platformKey}. Run \`npm run build\` to compile.`);
  console.warn(e instanceof Error ? e.message : String(e));
}

/**
 * Default options matching babel-preset-solid
 */
export const defaultOptions = {
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
export function transform(source, options = {}) {
  if (!nativeBinding) {
    throw new Error('solid-jsx-oxc: Native module not loaded. Ensure it is built for your platform.');
  }

  const mergedOptions = { ...defaultOptions, ...options };

  // NAPI-RS automatically converts camelCase (JS) to snake_case (Rust)
  // so we can pass options directly without manual conversion
  return nativeBinding.transformJsx(source, mergedOptions);
}

/**
 * Create a preset configuration (for compatibility with babel-preset-solid interface)
 * @param {object} context - Babel context (ignored, for compatibility)
 * @param {object} options - User options
 * @returns {object}
 */
export function preset(context, options = {}) {
  const mergedOptions = { ...defaultOptions, ...options };

  return {
    // Return the options that would be passed to the transform
    options: mergedOptions,

    // The transform function
    transform: (source) => transform(source, mergedOptions),
  };
}

/**
 * Low-level transform function from the native binding
 */
export const transformJsx = nativeBinding ? nativeBinding.transformJsx : null;

// Default export for convenience
export default {
  transform,
  preset,
  defaultOptions,
  transformJsx,
};
