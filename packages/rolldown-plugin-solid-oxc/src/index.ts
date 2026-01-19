/**
 * Rolldown plugin for SolidJS using OXC-based compiler
 *
 * Since Rolldown uses OXC internally, this provides optimal performance.
 * Uses Rolldown's native plugin hook filters for maximum efficiency.
 */

import type { Plugin } from 'rolldown';

export interface SolidOxcOptions {
  /**
   * Dev mode - enables additional debugging
   * @default false
   */
  dev?: boolean;

  /**
   * Hot module replacement (requires dev: true)
   * @default true in dev mode
   */
  hot?: boolean;

  /**
   * Filter which files to transform (regex pattern)
   * @default /\.[jt]sx$/
   */
  include?: RegExp;

  /**
   * Filter which files to exclude (regex pattern)
   * @default /node_modules/
   */
  exclude?: RegExp;

  /**
   * The module to import runtime helpers from
   * @default 'solid-js/web'
   */
  module_name?: string;

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
  delegate_events?: boolean;

  /**
   * Wrap conditionals in memos
   * @default true
   */
  wrap_conditionals?: boolean;

  /**
   * Pass context to custom elements
   * @default true
   */
  context_to_custom_elements?: boolean;

  /**
   * Built-in components that should be passed through
   */
  builtIns?: string[];

  /**
   * Enable SSR mode
   * @default false
   */
  ssr?: boolean;
}

const defaultOptions: SolidOxcOptions = {
  include: /\.[jt]sx$/,
  exclude: /node_modules/,
  module_name: 'solid-js/web',
  generate: 'dom',
  hydratable: false,
  delegate_events: true,
  wrap_conditionals: true,
  context_to_custom_elements: true,
  dev: false,
  hot: true,
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
 * Rolldown plugin for SolidJS using OXC-based compiler
 */
export default function solidOxc(options: SolidOxcOptions = {}): Plugin {
  const opts = { ...defaultOptions, ...options };

  // Lazy load the native module
  let solidJsxOxc: typeof import('solid-jsx-oxc') | null = null;

  return {
    name: 'rolldown-plugin-solid-oxc',

    async buildStart() {
      try {
        solidJsxOxc = await import('solid-jsx-oxc');
      } catch (e) {
        this.error(
          'Failed to load solid-jsx-oxc. Make sure it is built for your platform.'
        );
      }
    },

    // Use Rolldown's native hook filter for optimal performance
    // Rolldown skips calling the plugin entirely for non-matching files
    transform: {
      filter: {
        id: {
          include: opts.include,
          exclude: opts.exclude,
        },
      },
      async handler(code: string, id: string) {
        // Strip query parameters (e.g., ?v=123 from dev servers)
        const fileId = id.split('?', 1)[0];

        if (!solidJsxOxc) {
          this.error('solid-jsx-oxc module not loaded');
          return null;
        }

        const generate = opts.ssr ? 'ssr' : opts.generate;

        try {
          const result = solidJsxOxc.transformJsx(code, {
            filename: fileId,
            moduleName: opts.module_name,
            generate,
            hydratable: opts.hydratable,
            delegateEvents: opts.delegate_events,
            wrapConditionals: opts.wrap_conditionals,
            contextToCustomElements: opts.context_to_custom_elements,
            sourceMap: true,
          });

          let finalCode = result.code;

          // Add HMR support in dev mode
          if (opts.dev && opts.hot !== false) {
            const hotCode = `
if (import.meta.hot) {
  import.meta.hot.accept();
}
`;
            finalCode = finalCode + hotCode;
          }

          return {
            code: finalCode,
            map: result.map ? JSON.parse(result.map) : null,
          };
        } catch (e: unknown) {
          const message = e instanceof Error ? e.message : String(e);
          this.error(`Failed to transform ${id}: ${message}`);
          return null;
        }
      },
    },
  };
}

// Named export for compatibility
export { solidOxc };
