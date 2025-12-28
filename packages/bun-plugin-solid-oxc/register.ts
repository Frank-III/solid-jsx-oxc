/**
 * Register the Solid OXC plugin for Bun runtime
 *
 * Usage in bunfig.toml:
 * ```toml
 * preload = ["bun-plugin-solid-oxc/register"]
 * ```
 *
 * Or via CLI:
 * ```bash
 * bun --preload bun-plugin-solid-oxc/register ./src/index.tsx
 * ```
 */

import { plugin } from 'bun';
import solidOxc from './src/index';

plugin(solidOxc());
