import type { Plugin, FilterPattern } from 'vite';
import { createFilter } from 'vite';

// Will be imported from the NAPI bindings
// import { transform } from 'solid-jsx-oxc';

export interface SolidOxcOptions {
  /**
   * Filter which files to transform
   * @default /\.[jt]sx$/
   */
  include?: FilterPattern;

  /**
   * Filter which files to exclude
   * @default /node_modules/
   */
  exclude?: FilterPattern;

  /**
   * The module to import runtime helpers from
   * @default 'solid-js/web'
   */
  moduleName?: string;

  /**
   * Generate mode
   * @default 'dom'
   */
  generate?: 'dom' | 'ssr' | 'universal';

  /**
   * Enable hydration support
   * @default false
   */
  hydratable?: boolean;

  /**
   * Delegate events for better performance
   * @default true
   */
  delegateEvents?: boolean;

  /**
   * Wrap conditionals in memos
   * @default true
   */
  wrapConditionals?: boolean;

  /**
   * Pass context to custom elements
   * @default true
   */
  contextToCustomElements?: boolean;

  /**
   * Built-in components that should be passed through
   */
  builtIns?: string[];

  /**
   * Enable SSR mode (shorthand for generate: 'ssr')
   * @default false
   */
  ssr?: boolean;

  /**
   * Dev mode - enables additional debugging
   * @default based on vite mode
   */
  dev?: boolean;

  /**
   * Hot module replacement
   * @default true in dev mode
   */
  hot?: boolean;
}

const defaultOptions: SolidOxcOptions = {
  include: /\.[jt]sx$/,
  exclude: /node_modules/,
  moduleName: 'solid-js/web',
  generate: 'dom',
  hydratable: false,
  delegateEvents: true,
  wrapConditionals: true,
  contextToCustomElements: true,
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
    'ErrorBoundary',
  ],
};

/**
 * Vite plugin for SolidJS using OXC-based compiler
 */
export default function solidOxc(options: SolidOxcOptions = {}): Plugin {
  const opts = { ...defaultOptions, ...options };
  const filter = createFilter(opts.include, opts.exclude);

  let isDev = false;
  let isSSR = false;

  // Lazy load the native module
  let solidJsxOxc: typeof import('solid-jsx-oxc') | null = null;

  return {
    name: 'vite-plugin-solid-oxc',

    enforce: 'pre',

    configResolved(config) {
      isDev = config.command === 'serve';
      isSSR = opts.ssr ?? !!config.build?.ssr;
    },

    async buildStart() {
      // Load the native module
      try {
        solidJsxOxc = await import('solid-jsx-oxc');
      } catch (e) {
        this.error(
          'Failed to load solid-jsx-oxc. Make sure it is built for your platform.\n' +
          'Run: cd packages/solid-jsx-oxc && npm run build'
        );
      }
    },

    async transform(code, id) {
      if (!filter(id)) {
        return null;
      }

      if (!solidJsxOxc) {
        this.error('solid-jsx-oxc module not loaded');
        return null;
      }

      const generate = isSSR ? 'ssr' : opts.generate;

      try {
        const result = solidJsxOxc.transformJsx(code, {
          filename: id,
          moduleName: opts.moduleName,
          generate,
          hydratable: opts.hydratable,
          delegateEvents: opts.delegateEvents,
          wrapConditionals: opts.wrapConditionals,
          contextToCustomElements: opts.contextToCustomElements,
          sourceMap: true,
        });

        // Add HMR support in dev mode
        if (isDev && opts.hot !== false) {
          const hotCode = `
if (import.meta.hot) {
  import.meta.hot.accept();
}
`;
          result.code = result.code + hotCode;
        }

        return {
          code: result.code,
          map: result.map ? JSON.parse(result.map) : null,
        };
      } catch (e: any) {
        this.error(`Failed to transform ${id}: ${e.message}`);
        return null;
      }
    },

    // Handle Solid's JSX types
    config() {
      return {
        esbuild: {
          // Let our plugin handle JSX, not esbuild
          jsx: 'preserve',
          jsxImportSource: 'solid-js',
        },
        resolve: {
          conditions: ['solid'],
          dedupe: ['solid-js', 'solid-js/web'],
        },
        optimizeDeps: {
          // Include solid-js in pre-bundling
          include: ['solid-js', 'solid-js/web'],
          // Disable automatic discovery - we know what deps we need
          noDiscovery: true,
        },
      };
    },
  };
}

// Named export for compatibility
export { solidOxc };

// Type exports
export type { Plugin };
