#!/usr/bin/env node
// 本地打包脚本：版本号 patch +1，构建指定平台并归档到仓库根目录 release-<platform>/。
//
// 用法（仓库根目录）：
//   node scripts/build-release.mjs macos                 # 本机构建 macOS（app + dmg）
//   node scripts/build-release.mjs windows               # Windows x64 + ARM64
//   node scripts/build-release.mjs windows-x64
//   node scripts/build-release.mjs windows-arm64
//
// Windows 交叉编译（macOS host）：
//   - cargo-xwin（下载 Windows SDK/CRT）+ brew llvm 的 clang + 本机 makensis(brew nsis)
//   - ring 0.17.14 对 aarch64-pc-windows-msvc 硬编码普通 clang，会把 MSVC 风格
//     "/imsvc" 传给 macOS 上 POSIX 驱动的 clang 导致失败；这里用一个本地 wrapper
//     把 "/imsvc" 翻译成 "-isystem" 绕开（上游修复未发版）。
//   - 签名：读取环境变量 TAURI_SIGNING_PRIVATE_KEY_PATH；缺省时在
//     src-tauri/target/oq-build-tools 下生成一把一次性 minisign key 用于本地自测
//     （仅自测，勿用于发布）。
//
// 前置（Windows 一次性）：
//   brew install nsis llvm
//   rustup target add aarch64-pc-windows-msvc x86_64-pc-windows-msvc
//   cargo install --locked cargo-xwin
import { execFileSync, execSync } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const srcTauri = path.join(root, 'src-tauri');
const wrapperDir = path.join(srcTauri, 'target', 'oq-build-tools');
const clangWrapper = path.join(wrapperDir, 'clang');

if (process.platform !== 'darwin') {
  console.error('此脚本按 macOS 开发机编写（Windows 走 cargo-xwin 交叉编译）');
  process.exit(1);
}

const TARGETS = {
  'windows-x64': { target: 'x86_64-pc-windows-msvc', platform: 'windows' },
  'windows-arm64': { target: 'aarch64-pc-windows-msvc', platform: 'windows' },
  windows: null, // x64 + arm64
  macos: null,
};
const ARCHES = {
  'x86_64-pc-windows-msvc': 'x64',
  'aarch64-pc-windows-msvc': 'arm64',
};

const planArg = process.argv[2];
if (!planArg || !(planArg in TARGETS)) {
  console.error(`用法: node scripts/build-release.mjs <macos|windows|windows-x64|windows-arm64>`);
  process.exit(1);
}

function replaceRequired(content, pattern, replacement, label) {
  const updated = content.replace(pattern, replacement);
  if (updated === content) {
    throw new Error(`无法更新 ${label} 中的版本号`);
  }
  return updated;
}

function bumpPatchVersion() {
  const packageJsonPath = path.join(root, 'package.json');
  const tauriConfigPath = path.join(srcTauri, 'tauri.conf.json');
  const cargoManifestPath = path.join(srcTauri, 'Cargo.toml');
  const cargoLockPath = path.join(srcTauri, 'Cargo.lock');

  const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
  const tauriConfig = JSON.parse(readFileSync(tauriConfigPath, 'utf8'));
  const cargoManifest = readFileSync(cargoManifestPath, 'utf8');
  const cargoLock = readFileSync(cargoLockPath, 'utf8');
  const cargoVersion = cargoManifest.match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
  const versions = [packageJson.version, tauriConfig.version, cargoVersion];

  if (!cargoVersion || new Set(versions).size !== 1) {
    throw new Error(`打包前版本号不一致: package=${packageJson.version}, tauri=${tauriConfig.version}, cargo=${cargoVersion}`);
  }

  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(packageJson.version);
  if (!match) {
    throw new Error(`不支持自动递增的版本号: ${packageJson.version}`);
  }

  const currentVersion = packageJson.version;
  const nextVersion = `${match[1]}.${match[2]}.${Number(match[3]) + 1}`;
  const versionPattern = /^(\s*"version"\s*:\s*")[^"]+("\s*,?\s*$)/m;

  writeFileSync(
    packageJsonPath,
    replaceRequired(
      readFileSync(packageJsonPath, 'utf8'),
      versionPattern,
      (_, prefix, suffix) => `${prefix}${nextVersion}${suffix}`,
      'package.json',
    ),
  );
  writeFileSync(
    tauriConfigPath,
    replaceRequired(
      readFileSync(tauriConfigPath, 'utf8'),
      versionPattern,
      (_, prefix, suffix) => `${prefix}${nextVersion}${suffix}`,
      'tauri.conf.json',
    ),
  );
  writeFileSync(
    cargoManifestPath,
    replaceRequired(
      cargoManifest,
      /(\[package\][\s\S]*?^version\s*=\s*")[^"]+("\s*$)/m,
      (_, prefix, suffix) => `${prefix}${nextVersion}${suffix}`,
      'Cargo.toml',
    ),
  );
  writeFileSync(
    cargoLockPath,
    replaceRequired(
      cargoLock,
      /(name = "usage01"\nversion = ")[^"]+(")/,
      (_, prefix, suffix) => `${prefix}${nextVersion}${suffix}`,
      'Cargo.lock',
    ),
  );

  console.log(`版本号: ${currentVersion} -> ${nextVersion}`);
  return nextVersion;
}

const version = bumpPatchVersion();

function run(cmd, extraEnv) {
  console.log(`\n▶ ${cmd}`);
  execSync(cmd, { stdio: 'inherit', cwd: root, env: extraEnv, encoding: 'utf8' });
}

// 打包用签名 key：优先环境变量，否则生成一次性 key（仅本地自测）。
function resolveSigningKeys() {
  const keyPath = process.env.TAURI_SIGNING_PRIVATE_KEY_PATH;
  if (keyPath) {
    return { keyPath, keyPassword: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD };
  }
  mkdirSync(wrapperDir, { recursive: true });
  const generated = path.join(wrapperDir, 'testkey');
  if (!existsSync(generated)) {
    console.log('未设置 TAURI_SIGNING_PRIVATE_KEY_PATH，生成一次性打包 key …');
    run(`corepack pnpm exec tauri signer generate -w ${generated}`);
  }
  return { keyPath: generated, keyPassword: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD };
}

// 生成 clang wrapper：把 MSVC 风格 "/imsvc" 翻译成 POSIX 的 "-isystem"。
function ensureClangWrapper() {
  mkdirSync(wrapperDir, { recursive: true });
  writeFileSync(
    clangWrapper,
    `#!/bin/bash
declare -a out=()
for a in "$@"; do
  if [ "$a" = "/imsvc" ]; then out+=("-isystem"); else out+=("$a"); fi
done
exec /opt/homebrew/opt/llvm/bin/clang "\${out[@]}"
`,
  );
  chmodSync(clangWrapper, 0o755);
}

function buildWindows(target) {
  ensureClangWrapper();
  const env = {
    ...process.env,
    TAURI_SIGNING_PRIVATE_KEY_PATH: keys.keyPath,
    PATH: `${wrapperDir}:/opt/homebrew/opt/llvm/bin:${process.env.PATH || ''}`,
    XWIN_CACHE_DIR: process.env.XWIN_CACHE_DIR || path.join(process.env.HOME, '.cache', 'xwin'),
  };
  // 清理上次显式覆盖，避免干扰（wrapper 走 PATH 生效）
  delete env.CC_aarch64_pc_windows_msvc;
  delete env.CXX_aarch64_pc_windows_msvc;
  delete env.CC_x86_64_pc_windows_msvc;
  run(`corepack pnpm exec tauri build --runner cargo-xwin --target ${target} --bundles nsis`, env);

  const arch = ARCHES[target];
  const setupExe = path.join(
    srcTauri,
    `target/${target}/release/bundle/nsis/Usage01_${version}_${arch}-setup.exe`,
  );
  if (!existsSync(setupExe)) {
    console.error(`产物缺失：${setupExe}`);
    process.exit(1);
  }
  const dstDir = path.join(root, 'release-windows');
  mkdirSync(dstDir, { recursive: true });
  copyFileSync(setupExe, path.join(dstDir, path.basename(setupExe)));
  console.log(`✓ 已归档: ${path.join(dstDir, path.basename(setupExe))}`);

  // Portable：直接归档编译出的裸 exe（无需安装，便于快速自测）。
  const portableExe = path.join(srcTauri, `target/${target}/release/usage01.exe`);
  const portableName = `Usage01_${version}_${arch}-portable.exe`;
  copyFileSync(portableExe, path.join(dstDir, portableName));
  console.log(`✓ 已归档: ${path.join(dstDir, portableName)}`);
}

function buildMacos() {
  const env = {
    ...baseEnv(),
    APPLE_SIGNING_IDENTITY: process.env.APPLE_SIGNING_IDENTITY || '-',
  };
  run(`corepack pnpm exec tauri build --bundles app,dmg`, env);

  const dstDir = path.join(root, 'release-macos');
  mkdirSync(dstDir, { recursive: true });
  const copied = [];
  for (const [bundle, ext] of [
    ['dmg', '.dmg'],
    ['macos', '.app'],
  ]) {
    const dir = path.join(srcTauri, `target/release/bundle/${bundle}`);
    if (!existsSync(dir)) continue;
    for (const f of readdirSync(dir)) {
      if (f.endsWith(ext)) {
        const source = path.join(dir, f);
        const dest = path.join(dstDir, f);
        if (statSync(source).isDirectory()) {
          rmSync(dest, { recursive: true, force: true });
          execFileSync('ditto', [source, dest], { stdio: 'inherit' });
        } else {
          copyFileSync(source, dest);
        }
        copied.push(dest);
      }
    }
  }
  if (!copied.length) {
    console.error('未找到 macOS 归档产物');
    process.exit(1);
  }
  for (const c of copied) console.log(`✓ 已归档: ${c}`);
}

function baseEnv() {
  return {
    ...process.env,
    TAURI_SIGNING_PRIVATE_KEY_PATH: keys.keyPath,
  };
}

const keys = resolveSigningKeys();

if (planArg === 'macos') {
  buildMacos();
} else if (planArg === 'windows') {
  buildWindows('x86_64-pc-windows-msvc');
  buildWindows('aarch64-pc-windows-msvc');
} else {
  buildWindows(TARGETS[planArg].target);
}
console.log('\n完成。');
