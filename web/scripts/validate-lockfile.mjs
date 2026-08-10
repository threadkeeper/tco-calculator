import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const webRoot = fileURLToPath(new URL('../', import.meta.url));
const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
const lockfile = JSON.parse(readFileSync(new URL('../package-lock.json', import.meta.url), 'utf8'));
const errors = [];
const allowedHost = /^(?:packagefeedproxy\.microsoft\.io|ms-feed-\d+\.pkgs\.visualstudio\.com)$/;
const exactVersion = /^\d+\.\d+\.\d+$/;
const prohibitedLicense = /\b(?:AGPL|GPL|SSPL)(?:-|\b)|Commons Clause/i;
let missingIntegrityCount = 0;
let legacyIntegrityCount = 0;

if (manifest.packageManager !== 'npm@11.17.0') {
  errors.push('packageManager must be npm@11.17.0.');
}
if (lockfile.lockfileVersion !== 3) {
  errors.push('package-lock.json must use lockfileVersion 3.');
}

for (const dependencyGroup of ['dependencies', 'devDependencies', 'overrides']) {
  for (const [name, version] of Object.entries(manifest[dependencyGroup] ?? {})) {
    if (typeof version !== 'string' || !exactVersion.test(version)) {
      errors.push(`${dependencyGroup}.${name} must use an exact stable version.`);
    }
  }
}

const packages = Object.entries(lockfile.packages ?? {}).filter(([path]) => path !== '');
for (const [path, metadata] of packages) {
  if (metadata.hasInstallScript === true) {
    errors.push(`${path} declares a lifecycle install script.`);
  }
  if (typeof metadata.license !== 'string' || metadata.license.length === 0) {
    errors.push(`${path} has no lockfile license metadata.`);
  } else if (prohibitedLicense.test(metadata.license)) {
    errors.push(`${path} declares prohibited license metadata: ${metadata.license}.`);
  }
  if (typeof metadata.resolved === 'string') {
    const resolved = new URL(metadata.resolved);
    if (resolved.protocol !== 'https:' || !allowedHost.test(resolved.hostname)) {
      errors.push(`${path} resolves outside the Microsoft npm proxy: ${resolved.hostname}.`);
    }
    if (metadata.integrity == null) {
      missingIntegrityCount += 1;
    } else if (
      typeof metadata.integrity !== 'string' ||
      !/^sha(?:1|256|384|512)-[A-Za-z0-9+/]+={0,2}$/.test(metadata.integrity)
    ) {
      errors.push(`${path} has malformed SRI integrity metadata.`);
    } else if (!metadata.integrity.startsWith('sha512-')) {
      legacyIntegrityCount += 1;
    }
  }
}

if (errors.length > 0) {
  console.error(`Frontend dependency policy failed for ${webRoot}:`);
  for (const error of errors) console.error(`- ${error}`);
  process.exitCode = 1;
} else {
  console.log(
    `Validated ${packages.length} locked package entries through the Microsoft npm proxy.`
  );
  if (missingIntegrityCount > 0) {
    console.log(
      `${missingIntegrityCount} entries use exact proxy URLs without upstream integrity metadata supplied by the proxy.`
    );
  }
  if (legacyIntegrityCount > 0) {
    console.log(`${legacyIntegrityCount} entries use valid non-SHA512 SRI supplied by the proxy.`);
  }
}
