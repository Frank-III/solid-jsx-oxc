/**
 * solid-jsx-oxc - OXC-based JSX compiler for SolidJS (ESM)
 */

import { createRequire } from 'module';
import { platform, arch } from 'process';

const require = createRequire(import.meta.url);

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
    console.warn('solid-jsx-oxc: Native module not found. Run `npm run build` to compile.');
  }
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
  generate: 'dom',
  hydratable: false,
  delegateEvents: true,
  sourceMap: false,
};

/**
 * Transform JSX source code
 */
export function transform(source, options = {}) {
  if (!nativeBinding) {
    throw new Error('solid-jsx-oxc: Native module not loaded.');
  }

  const mergedOptions = { ...defaultOptions, ...options };
  return nativeBinding.transformJsx(source, mergedOptions);
}

/**
 * Create a preset configuration
 */
export function preset(context, options = {}) {
  const mergedOptions = { ...defaultOptions, ...options };

  return {
    options: mergedOptions,
    transform: (source) => transform(source, mergedOptions),
  };
}

/**
 * Direct export of native transformJsx function
 */
export function transformJsx(source, options = {}) {
  if (!nativeBinding) {
    throw new Error('solid-jsx-oxc: Native module not loaded.');
  }
  return nativeBinding.transformJsx(source, options);
}

export default {
  transform,
  preset,
  transformJsx,
  defaultOptions,
};
