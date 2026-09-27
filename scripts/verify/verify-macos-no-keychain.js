import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const rustRoot = path.join(root, 'src-tauri/src');

const fail = (message) => {
  throw new Error(message);
};

const maskRange = (chars, start, end) => {
  for (let i = start; i < end; i += 1) {
    if (chars[i] !== '\n') chars[i] = ' ';
  }
};

// Remove Rust comments while retaining literals and line positions. Keeping literals lets the
// scanner find an actual shell command or API string while docs and source comments cannot trip it.
function withoutComments(source) {
  const chars = source.split('');
  let i = 0;
  while (i < source.length) {
    if (source.startsWith('//', i)) {
      const end = source.indexOf('\n', i);
      const stop = end === -1 ? source.length : end;
      for (let j = i; j < stop; j += 1) chars[j] = ' ';
      i = stop;
      continue;
    }
    if (source.startsWith('/*', i)) {
      let depth = 1;
      const start = i;
      i += 2;
      while (i < source.length && depth > 0) {
        if (source.startsWith('/*', i)) {
          depth += 1;
          i += 2;
        } else if (source.startsWith('*/', i)) {
          depth -= 1;
          i += 2;
        } else {
          i += 1;
        }
      }
      for (let j = start; j < i; j += 1) {
        if (chars[j] !== '\n') chars[j] = ' ';
      }
      continue;
    }
    if (source[i] === '"' || (source[i] === 'b' && source[i + 1] === '"')) {
      if (source[i] === 'b') i += 1;
      i = skipQuoted(source, i, '"');
      continue;
    }
    const rawStart = rawStringEnd(source, i);
    if (rawStart !== null) {
      i = rawStart;
      continue;
    }
    i += 1;
  }
  return chars.join('');
}

function rawStringEnd(source, start) {
  let quote = start;
  if (source[quote] === 'b' && source[quote + 1] === 'r') quote += 1;
  if (source[quote] !== 'r') return null;
  quote += 1;
  while (source[quote] === '#') quote += 1;
  if (source[quote] !== '"') return null;
  const hashes = source.slice(start, quote).match(/#/g)?.length ?? 0;
  const close = `"${'#'.repeat(hashes)}`;
  const end = source.indexOf(close, quote + 1);
  return end === -1 ? source.length : end + close.length;
}

function skipQuoted(source, start, quote) {
  let i = start + 1;
  while (i < source.length) {
    if (source[i] === '\\') i += 2;
    else if (source[i] === quote) return i + 1;
    else i += 1;
  }
  return source.length;
}

function skipRustLiteral(source, start) {
  const rawEnd = rawStringEnd(source, start);
  if (rawEnd !== null) return rawEnd;
  if (source[start] === 'b' && source[start + 1] === '"') return skipQuoted(source, start + 1, '"');
  if (source[start] === '"') return skipQuoted(source, start, '"');
  if (source[start] === 'b' && source[start + 1] === "'") return skipQuoted(source, start + 1, "'");
  if (source[start] === "'") {
    // A lifetime starts with an apostrophe followed by an identifier, not a char literal.
    if (/^'[A-Za-z_][A-Za-z0-9_]*\b/.test(source.slice(start))) return start + 1;
    return skipQuoted(source, start, "'");
  }
  return start;
}

function findItemEnd(source, start) {
  let braceDepth = 0;
  let sawBody = false;
  for (let i = start; i < source.length;) {
    const literalEnd = skipRustLiteral(source, i);
    if (literalEnd !== i) {
      i = literalEnd;
      continue;
    }
    if (source[i] === '{') {
      braceDepth += 1;
      sawBody = true;
    } else if (source[i] === '}') {
      braceDepth -= 1;
      if (sawBody && braceDepth === 0) return i + 1;
    } else if (source[i] === ';' && braceDepth === 0) {
      return i + 1;
    }
    i += 1;
  }
  return source.length;
}

function cfgEnabled(expression) {
  const tokens =
    expression.match(/\s*(?:[A-Za-z_][A-Za-z0-9_]*|"(?:\\.|[^"\\])*"|[(),=])\s*/g) ?? [];
  const text = tokens.join('').replace(/\s+/g, '');
  let index = 0;
  const parse = () => {
    const identifier = text.slice(index).match(/^[A-Za-z_][A-Za-z0-9_]*/)?.[0];
    if (!identifier) throw new Error(`Unsupported cfg expression: ${expression}`);
    index += identifier.length;
    if (identifier === 'target_os' && text[index] === '=') {
      index += 1;
      const value = text.slice(index).match(/^"([^"\\]*)"/)?.[1];
      if (!value) throw new Error(`Unsupported cfg expression: ${expression}`);
      index += value.length + 2;
      return value === 'macos';
    }
    if (text[index] !== '(') {
      if (identifier === 'test' || identifier === 'windows') return false;
      if (identifier === 'unix') return true;
      return null;
    }
    index += 1;
    const values = [];
    while (text[index] !== ')') {
      values.push(parse());
      if (text[index] === ',') index += 1;
      else if (text[index] !== ')') throw new Error(`Unsupported cfg expression: ${expression}`);
    }
    index += 1;
    if (identifier === 'not' && values.length === 1) return values[0] === null ? null : !values[0];
    if (identifier === 'all')
      return values.includes(false) ? false : values.includes(null) ? null : true;
    if (identifier === 'any')
      return values.includes(true) ? true : values.includes(null) ? null : false;
    return null;
  };
  const result = parse();
  return index === text.length ? result : null;
}

function removeNonMacItems(source) {
  const cfgLine = /^([ \t]*)#\[cfg\(([^\n]*)\)\][ \t]*$/gm;
  let match;
  const ranges = [];
  while ((match = cfgLine.exec(source)) !== null) {
    if (cfgEnabled(match[2].trim()) !== false) continue;
    let start = cfgLine.lastIndex;
    while (start < source.length && /\s/.test(source[start])) start += 1;
    // Rust permits more attributes between cfg and the item.
    while (source.startsWith('#[', start)) {
      const close = source.indexOf(']', start + 2);
      if (close === -1) break;
      start = close + 1;
      while (start < source.length && /\s/.test(source[start])) start += 1;
    }
    const end = findItemEnd(source, start);
    ranges.push([match.index, end]);
  }
  const chars = source.split('');
  for (const [start, end] of ranges) maskRange(chars, start, end);
  return chars.join('');
}

function rustFiles(directory) {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (!['tests', 'fixtures', 'testdata', 'benches'].includes(entry.name))
        files.push(...rustFiles(full));
    } else if (
      entry.isFile() &&
      entry.name.endsWith('.rs') &&
      !/(?:^|_)tests?\.rs$/.test(entry.name)
    ) {
      files.push(full);
    }
  }
  return files.sort();
}

const sourceRules = [
  ['Security.framework import or module', /\bsecurity_framework\b/g],
  [
    'Keychain SecItem credential API',
    /\b(?:SecItem[A-Za-z0-9_]*|kSecClassGenericPassword|SecKeychain[A-Za-z0-9_]*)\b/g,
  ],
  [
    'generic-password credential API call',
    /\b(?:read|write|delete|find|search|copy|add|update)_generic_password\s*\(|\bgeneric_password_(?:service_exists|exists)\s*\(/g,
  ],
  ['macOS keychain-access grant path', /\bkeychain_access\s*::\s*(?:is_granted|sync)\s*\(/g],
  [
    'Keychain-specific provider source variant',
    /\b(?:AuthSource|CredentialSource|TokenSource)\s*::\s*Keychain\b|^\s*Keychain(?:\s*[({,]|$)/gm,
  ],
  [
    'GitHub CLI credential command',
    /\.args\s*\(\s*\[\s*['"]auth['"]\s*,\s*['"]token['"]|\.arg\(\s*['"]auth['"]\s*\)\s*\.arg\(\s*['"]token['"]|\bgh\s+auth\s+token\b/g,
  ],
];

const isFunctionDefinition = (source, index) =>
  /\bfn\s+$/.test(source.slice(Math.max(0, index - 16), index));

const hits = [];
for (const file of rustFiles(rustRoot)) {
  const relative = path.relative(root, file).split(path.sep).join('/');
  const original = fs.readFileSync(file, 'utf8');
  const uncommented = withoutComments(original);
  const source = removeNonMacItems(uncommented);
  for (const [label, pattern] of sourceRules) {
    pattern.lastIndex = 0;
    let match;
    while ((match = pattern.exec(source)) !== null) {
      const line = source.slice(0, match.index).split('\n').length;
      if (
        label === 'generic-password credential API call' &&
        isFunctionDefinition(source, match.index)
      )
        continue;
      hits.push(`${relative}:${line}: ${label}: ${match[0].replace(/\s+/g, ' ')}`);
      if (match[0].length === 0) pattern.lastIndex += 1;
    }
  }
}
if (hits.length)
  fail(`macOS-reachable production Rust contains credential-store access:\n${hits.join('\n')}`);

const manifest = fs.readFileSync(path.join(root, 'src-tauri/Cargo.toml'), 'utf8');
const directFramework = /^\s*(?:security-framework|security_framework)\s*=/m.exec(manifest);
if (directFramework) {
  const line = manifest.slice(0, directFramework.index).split('\n').length;
  fail(`src-tauri/Cargo.toml:${line}: direct Security.framework dependency is forbidden`);
}

const metadataResult = spawnSync(
  'cargo',
  [
    'metadata',
    '--manifest-path',
    'src-tauri/Cargo.toml',
    '--format-version',
    '1',
    '--locked',
    '--filter-platform',
    'aarch64-apple-darwin',
  ],
  { cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 },
);
if (metadataResult.status !== 0) {
  fail(`cargo metadata failed: ${metadataResult.error?.message ?? metadataResult.stderr.trim()}`);
}
const metadata = JSON.parse(metadataResult.stdout);
const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
const framework = [...metadata.packages].find((pkg) => pkg.name === 'security-framework');
if (framework && nodes.has(framework.id)) {
  const incoming = new Map();
  for (const node of nodes.values()) {
    for (const dep of node.deps) {
      const list = incoming.get(dep.pkg) ?? [];
      list.push(node.id);
      incoming.set(dep.pkg, list);
    }
  }
  const paths = [];
  const visit = (id, reversePath, seen) => {
    if (seen.has(id)) return;
    if (id === metadata.resolve.root) {
      paths.push([...reversePath, packages.get(id)?.name ?? id].reverse());
      return;
    }
    for (const parent of incoming.get(id) ?? []) {
      visit(parent, [...reversePath, packages.get(id)?.name ?? id], new Set([...seen, id]));
    }
  };
  visit(framework.id, [], new Set());
  if (paths.length === 0)
    fail('security-framework is in macOS packages but no resolved path to the app root was found');
  const disallowed = paths.filter(
    (chain) => !chain.includes('rustls-platform-verifier') || !chain.includes('reqwest'),
  );
  if (disallowed.length) {
    fail(
      `security-framework has a non-TLS macOS dependency path:\n${disallowed.map((chain) => `  ${chain.join(' -> ')}`).join('\n')}`,
    );
  }
  console.log(
    `Allowed TLS trust dependency: ${paths.map((chain) => chain.join(' -> ')).join('; ')}`,
  );
} else {
  console.log('No security-framework package is reachable in the macOS target graph.');
}

const windowsCredentialApi =
  /\[target\.'cfg\(target_os\s*=\s*"windows"\)'\.dependencies\][\s\S]*?windows-sys\s*=\s*\{[^}]*Win32_Security_Credentials/s.test(
    manifest,
  );
if (!windowsCredentialApi)
  fail('src-tauri/Cargo.toml: Windows Credential Manager dependency was removed');
const linuxSecretService =
  /\[target\.'cfg\(target_os\s*=\s*"linux"\)'\.dependencies\][\s\S]*?secret-service\s*=/s.test(
    manifest,
  );
if (!linuxSecretService) fail('src-tauri/Cargo.toml: Linux Secret Service dependency was removed');

console.log(
  `macOS credential-access scan passed (${rustFiles(rustRoot).length} production Rust files).`,
);
console.log('Windows Credential Manager and Linux Secret Service dependencies remain configured.');
