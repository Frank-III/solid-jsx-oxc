#!/usr/bin/env node
/**
 * Verification script for solid-jsx-oxc
 * Tests that the native module loads and transforms work correctly
 */

import { existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..');

console.log('🔍 Verifying solid-jsx-oxc...\n');

// Check binary exists
const platform = process.platform;
const arch = process.arch;
const platformMap = {
  'darwin-arm64': 'darwin-arm64',
  'darwin-x64': 'darwin-x64',
  'linux-x64': 'linux-x64-gnu',
  'linux-arm64': 'linux-arm64-gnu',
  'win32-x64': 'win32-x64-msvc',
  'win32-arm64': 'win32-arm64-msvc',
};

const target = platformMap[`${platform}-${arch}`] || 'unknown';
const binaryPath = join(root, `solid-jsx-oxc.${target}.node`);

console.log(`Platform: ${platform}-${arch}`);
console.log(`Binary: solid-jsx-oxc.${target}.node`);

if (!existsSync(binaryPath)) {
  console.error(`\n❌ Binary not found: ${binaryPath}`);
  console.error('   Run: bun run build');
  process.exit(1);
}
console.log('✅ Binary exists\n');

// Test loading
console.log('Testing module load...');
let mod;
try {
  mod = await import('../index.js');
  console.log('✅ Module loaded\n');
} catch (e) {
  console.error('❌ Failed to load module:', e.message);
  process.exit(1);
}

// Test transforms
const tests = [
  {
    name: 'Basic element',
    input: '<div class="hello">world</div>',
    expect: ['template', 'div'],
  },
  {
    name: 'Dynamic child',
    input: '<div>{count()}</div>',
    expect: ['insert', 'count()'],
  },
  {
    name: 'Event handler',
    input: '<button onClick={handler}>Click</button>',
    expect: ['template', 'delegateEvents'],
  },
  {
    name: 'Component',
    input: '<Button onClick={handler}>Click me</Button>',
    expect: ['createComponent', 'Button'],
  },
  {
    name: 'For loop',
    input: '<For each={items}>{item => <li>{item}</li>}</For>',
    expect: ['createComponent', 'For'],
  },
  {
    name: 'SSR mode',
    input: '<div class="hello">{name}</div>',
    options: { generate: 'ssr' },
    expect: ['ssr', 'escape'],
  },
];

let passed = 0;
let failed = 0;

console.log('Running transform tests...\n');

for (const test of tests) {
  try {
    const result = mod.transform(test.input, test.options || {});
    const missing = test.expect.filter(s => !result.code.includes(s));
    
    if (missing.length > 0) {
      console.log(`❌ ${test.name}`);
      console.log(`   Missing in output: ${missing.join(', ')}`);
      console.log(`   Output: ${result.code.slice(0, 100)}...`);
      failed++;
    } else {
      console.log(`✅ ${test.name}`);
      passed++;
    }
  } catch (e) {
    console.log(`❌ ${test.name}`);
    console.log(`   Error: ${e.message}`);
    failed++;
  }
}

console.log(`\n${passed} passed, ${failed} failed`);

if (failed > 0) {
  process.exit(1);
}

console.log('\n✨ All verifications passed!');
