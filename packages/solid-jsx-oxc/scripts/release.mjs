#!/usr/bin/env node
/**
 * Release script for solid-jsx-oxc
 *
 * Usage:
 *   bun run release:alpha           # Publish as alpha
 *   bun run release:alpha --dry-run # Dry run only
 *   bun run release:alpha --bump    # Bump version and publish
 *   bun run release:beta            # Publish as beta
 *   bun run release                 # Publish as latest
 */

import { execSync, spawn } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
const tag = args.find(a => !a.startsWith('--')) || 'latest';
const dryRun = args.includes('--dry-run');
const bump = args.includes('--bump');

const validTags = ['alpha', 'beta', 'latest', 'next'];

if (!validTags.includes(tag)) {
  console.error(`Invalid tag: ${tag}. Must be one of: ${validTags.join(', ')}`);
  process.exit(1);
}

function run(cmd, options = {}) {
  console.log(`\n> ${cmd}`);
  try {
    return execSync(cmd, { stdio: 'inherit', ...options });
  } catch (e) {
    if (!options.allowFail) {
      console.error(`\nCommand failed: ${cmd}`);
      process.exit(1);
    }
  }
}

function runCapture(cmd) {
  return execSync(cmd, { encoding: 'utf-8' }).trim();
}

async function prompt(question) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  return new Promise(resolve => {
    rl.question(question, answer => {
      rl.close();
      resolve(answer);
    });
  });
}

/**
 * Bump version for prerelease tags (alpha, beta, next)
 * e.g., 0.1.0-alpha.1 -> 0.1.0-alpha.2
 */
function bumpVersion(version, tag) {
  const prereleaseMatch = version.match(/^(.+)-(\w+)\.(\d+)$/);
  if (prereleaseMatch && prereleaseMatch[2] === tag) {
    // Same tag, increment number
    const [, base, , num] = prereleaseMatch;
    return `${base}-${tag}.${parseInt(num) + 1}`;
  }
  // Different tag or no prerelease, start at .1
  const baseVersion = version.replace(/-.*$/, '');
  return `${baseVersion}-${tag}.1`;
}

async function main() {
  console.log('🚀 solid-jsx-oxc Release Script\n');
  console.log(`Tag: ${tag}${dryRun ? ' (dry-run)' : ''}${bump ? ' (version bump)' : ''}`);

  // Read package.json
  let pkg = JSON.parse(readFileSync('package.json', 'utf-8'));

  // Handle version bump
  if (bump && tag !== 'latest') {
    const oldVersion = pkg.version;
    const newVersion = bumpVersion(oldVersion, tag);
    console.log(`\n📊 Version bump: ${oldVersion} -> ${newVersion}`);
    pkg.version = newVersion;
    writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
  }

  // Step 1: Run Rust tests
  console.log('\n📋 Step 1: Running Rust tests...');
  run('cargo test');

  // Step 2: Build native module
  console.log('\n🔨 Step 2: Building native module...');
  run('bun run build');

  // Step 3: Verify the module works
  console.log('\n✅ Step 3: Verifying module...');
  try {
    const { transform } = await import('../index.js');
    const result = transform('<div class="test">{count()}</div>');
    if (!result.code.includes('template')) {
      throw new Error('Transform output missing expected content');
    }
    console.log('   Module verification passed!');
  } catch (e) {
    console.error('   Module verification failed:', e.message);
    process.exit(1);
  }

  // Step 4: Show package info
  console.log('\n📦 Step 4: Package info');
  pkg = JSON.parse(readFileSync('package.json', 'utf-8')); // Re-read in case of bump
  console.log(`   Name: ${pkg.name}`);
  console.log(`   Version: ${pkg.version}`);

  // Step 5: Dry run
  console.log('\n📝 Step 5: Dry run...');
  run('npm pack --dry-run');

  if (dryRun) {
    console.log('\n🏁 Dry run complete. No publish performed.');
    process.exit(0);
  }

  // Step 6: Confirm and publish
  const confirm = await prompt(`\nPublish ${pkg.name}@${pkg.version} with tag '${tag}'? (y/N) `);
  if (confirm.toLowerCase() !== 'y') {
    console.log('Aborted.');
    process.exit(0);
  }

  // Step 7: Publish (npm will prompt for OTP if needed)
  console.log('\n🚢 Step 6: Publishing...');
  const child = spawn('npm', ['publish', '--tag', tag, '--access', 'public'], {
    stdio: 'inherit'
  });

  child.on('close', code => {
    if (code === 0) {
      console.log(`\n✨ Successfully published ${pkg.name}@${pkg.version} with tag '${tag}'`);
      console.log(`   Install with: npm install ${pkg.name}@${tag}`);
    } else {
      console.error('\n❌ Publish failed');
      process.exit(1);
    }
  });
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
