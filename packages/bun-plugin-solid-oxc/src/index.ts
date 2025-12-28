/**
 * Bun plugin for SolidJS using OXC-based compiler
 *
 * Since both Bun and this plugin use native code, this provides optimal performance.
 *
 * Usage with Bun.build():
 * ```ts
 * import solidPlugin from 'bun-plugin-solid-oxc';
 *
 * await Bun.build({
 *   entrypoints: ['./src/index.tsx'],
 *   outdir: './dist',
 *   plugins: [solidPlugin()],
 * });
 * ```
 *
 * Usage with bunfig.toml (runtime):
 * ```toml
 * [bunfig]
 * preload = ["bun-plugin-solid-oxc/register"]
 * ```
 */

import type { BunPlugin } from 'bun';

export interface SolidOxcOptions {
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
   * Enable SSR mode
   * @default false
   */
  ssr?: boolean;
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
};

/**
 * Bun plugin for SolidJS using OXC-based compiler
 */
export default function solidOxc(options: SolidOxcOptions = {}): BunPlugin {
  const opts = { ...defaultOptions, ...options };

  return {
    name: 'bun-plugin-solid-oxc',

    async setup(build) {
      // Load the native module once
      const solidJsxOxc = await import('solid-jsx-oxc');

      // Use Bun's onLoad hook with filter
      build.onLoad({ filter: opts.include! }, async (args) => {
        // Skip excluded files
        if (opts.exclude?.test(args.path)) {
          return undefined;
        }

        // Read the source file
        const source = await Bun.file(args.path).text();

        const generate = opts.ssr ? 'ssr' : opts.generate;

        try {
          const result = solidJsxOxc.transformJsx(source, {
            filename: args.path,
            moduleName: opts.moduleName,
            generate,
            hydratable: opts.hydratable,
            delegateEvents: opts.delegateEvents,
            wrapConditionals: opts.wrapConditionals,
            contextToCustomElements: opts.contextToCustomElements,
            sourceMap: false, // Bun handles source maps
          });

          // Determine loader based on file extension
          const loader = args.path.endsWith('.tsx') ? 'tsx' :
                        args.path.endsWith('.ts') ? 'ts' :
                        args.path.endsWith('.jsx') ? 'jsx' : 'js';

          return {
            contents: result.code,
            loader,
          };
        } catch (e: unknown) {
          const message = e instanceof Error ? e.message : String(e);
          throw new Error(`Failed to transform ${args.path}: ${message}`);
        }
      });
    },
  };
}

// Named export for compatibility
export { solidOxc };
