declare module 'rolldown' {
  export interface PluginContext {
    error(message: string): never;
  }

  export interface TransformFilter {
    id?: {
      include?: RegExp;
      exclude?: RegExp;
    };
  }

  export interface TransformResult {
    code: string;
    map?: any;
  }

  export interface Plugin {
    name: string;
    buildStart?(this: PluginContext): void | Promise<void>;
    transform?:
      | ((
          this: PluginContext,
          code: string,
          id: string
        ) => TransformResult | null | Promise<TransformResult | null>)
      | {
          filter?: TransformFilter;
          handler(
            this: PluginContext,
            code: string,
            id: string
          ): TransformResult | null | Promise<TransformResult | null>;
        };
  }
}

