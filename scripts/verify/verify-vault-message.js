import fs from 'node:fs';

const root = new URL('../../', import.meta.url);

// `providers::credential_vault::UNRECOVERABLE_VAULT_ERROR` reaches the UI as a
// plain string and is translated through an exact-match glossary entry. Neither
// side references the other, so a reworded Rust constant would silently drop the
// translation and fall back to English. This guard is the only thing tying them
// together.
const vaultSource = fs.readFileSync(
  new URL('src-tauri/src/providers/credential_vault.rs', root),
  'utf8',
);
const declared = vaultSource.match(
  /pub const UNRECOVERABLE_VAULT_ERROR: &str = "((?:[^"\\]|\\.)*)";/,
)?.[1];
if (declared === undefined) {
  throw new Error('credential_vault.rs does not declare UNRECOVERABLE_VAULT_ERROR.');
}
const message = declared.replace(/\\"/g, '"');

const glossarySource = fs.readFileSync(new URL('src/lib/i18n/backendGlossary.ts', root), 'utf8');
if (!glossarySource.includes(`'${message}':`)) {
  throw new Error(
    'backendGlossary.ts has no exact-match entry for the credential-vault message. ' +
      `Expected a key equal to the Rust constant: ${JSON.stringify(message)}`,
  );
}

// Every locale must carry the target key, or the message degrades to English.
const messagesDirectory = new URL('src/lib/i18n/messages/', root);
const localeFiles = fs.readdirSync(messagesDirectory).filter((name) => name.endsWith('.ts'));
for (const name of localeFiles) {
  const source = fs.readFileSync(new URL(name, messagesDirectory), 'utf8');
  if (!/^\s*credentialVaultUnrecoverable:/m.test(source)) {
    throw new Error(`${name} is missing providerError.credentialVaultUnrecoverable.`);
  }
}

console.log(
  `The credential-vault message matches its glossary entry and all ${localeFiles.length} locales translate it.`,
);
