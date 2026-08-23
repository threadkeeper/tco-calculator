import { once } from 'node:events';
import { access, mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

import { chromium } from '@playwright/test';

const calculatorUrl = 'https://azure.microsoft.com/en-us/pricing/calculator/';
const calculatorOrigin = 'https://azure.microsoft.com';
const liveOptIn = '--live-calculator';
const controlledOnly = process.argv.includes('--controlled-only');
const syntheticEstimateTitle = 'Workload 3';
const sqlMiItems = [
  { name: 'VM1', vcores: '32', memory: '256', storageUnits: '226' },
  { name: 'VM2', vcores: '4', memory: '28', storageUnits: '32' },
  { name: 'VM3', vcores: '4', memory: '28', storageUnits: '19' },
  { name: 'VM4', vcores: '4', memory: '32', storageUnits: '22' },
  { name: 'VM5', vcores: '4', memory: '32', storageUnits: '26' },
  { name: 'VM6', vcores: '16', memory: '128', storageUnits: '219' },
  { name: 'VM7', vcores: '8', memory: '64', storageUnits: '19' }
];

if (!process.argv.includes(liveOptIn)) {
  console.error(
    `This opt-in spike opens the live Azure Pricing Calculator with synthetic data. Re-run with ${liveOptIn}.`
  );
  process.exitCode = 2;
} else {
  await runSpike();
}

async function runSpike() {
  if (process.platform !== 'win32') {
    throw new Error('The Edge profile handoff spike supports Windows only.');
  }

  const edgeExecutable = await findEdgeExecutable();
  const profileDirectory = await mkdtemp(path.join(os.tmpdir(), 'tco-calculator-edge-'));
  let context;

  try {
    console.log('Stage 1/4: opening isolated Playwright Edge with synthetic data only.');
    context = await chromium.launchPersistentContext(profileDirectory, {
      acceptDownloads: false,
      channel: 'msedge',
      headless: false,
      locale: 'en-US'
    });

    const page = context.pages()[0] ?? (await context.newPage());
    await page.goto(calculatorUrl, { waitUntil: 'domcontentloaded', timeout: 60_000 });
    await assertCalculatorPage(page);

    console.log('Stage 2/4: naming the estimate and verifying seven synthetic SQL MI lines.');
    await setEstimateTitle(page);
    await createSqlMiItems(page);
    await verifyEstimateTitle(page);
    await verifySqlMiItems(page);

    await page.reload({ waitUntil: 'domcontentloaded', timeout: 60_000 });
    await assertCalculatorPage(page);
    await verifyEstimateTitle(page);
    await verifySqlMiItems(page);

    console.log('Stage 3/4: closing Playwright and its controlled Edge process.');
    await context.close();
    context = undefined;

    if (controlledOnly) {
      console.log('Controlled-only validation completed; ordinary Edge was not started.');
      return;
    }

    console.log(
      `Stage 4/4: opening ordinary Edge. Verify ${syntheticEstimateTitle} and the seven lines, sign in manually if approved, then close Edge.`
    );
    const exitCode = await launchOrdinaryEdge(edgeExecutable, profileDirectory);
    if (exitCode !== 0) {
      throw new Error(`Ordinary Edge exited with code ${exitCode}.`);
    }

    console.log('Synthetic handoff completed; removing the isolated profile.');
  } finally {
    if (context) {
      await context.close();
    }
    await rm(profileDirectory, { force: true, maxRetries: 3, recursive: true, retryDelay: 200 });
  }
}

async function assertCalculatorPage(page) {
  const url = new URL(page.url());
  if (url.origin !== calculatorOrigin || !url.pathname.startsWith('/en-us/pricing/calculator')) {
    throw new Error('Calculator navigation reached an unexpected origin or path.');
  }

  await page
    .locator('input[aria-label="Search products"]:visible')
    .first()
    .waitFor({ state: 'visible', timeout: 30_000 });
}

async function setEstimateTitle(page) {
  const title = page.getByPlaceholder('Your Estimate').first();
  await title.waitFor({ state: 'visible', timeout: 30_000 });
  await fillAndVerify(title, syntheticEstimateTitle);
}

async function verifyEstimateTitle(page) {
  const title = page.getByPlaceholder('Your Estimate').first();
  await title.waitFor({ state: 'visible', timeout: 30_000 });
  await expectValue(title, syntheticEstimateTitle);
}

async function createSqlMiItems(page) {
  const search = page.locator('input[aria-label="Search products"]:visible').first();
  await search.fill('SQL Managed Instance');

  const addButton = page.getByRole('button', { exact: true, name: 'Add to estimate' }).first();
  await addButton.waitFor({ state: 'visible', timeout: 30_000 });

  for (const [index, item] of sqlMiItems.entries()) {
    await addButton.click();
    const module = page.locator('[data-testid="azure-sql-module"]').nth(index);
    await module.waitFor({ state: 'visible', timeout: 30_000 });
    await configureSqlMiItem(module, item);
  }
}

async function configureSqlMiItem(module, item) {
  await fillAndVerify(module.locator('input[name="displayName"]'), item.name);
  await selectAndVerify(module.locator('select[name="region"]'), 'sweden-central');
  await selectAndVerify(module.locator('select[name="vcoreTier"]'), 'next-gen-general-purpose');
  await selectAndVerify(module.locator('select[name="generation"]'), 'premium-series');
  await selectAndVerify(module.locator('select[name="instanceSize"]'), item.vcores);
  await selectAndVerify(module.locator('select[name="ramMemory"]'), item.memory);
  await selectAndVerify(module.locator('select[name="recovery"]'), 'primaryinstance');
  await selectAndVerify(module.locator('select[name="zoneRedundancy"]'), 'local');
  await fillAndVerify(module.locator('input[name="managedCount"]'), '1');
  await fillAndVerify(module.locator('input[name="hours"]'), '730');
  await checkAndVerify(module.locator('input[name$="-databaseBillingOption"][value="payg"]'));
  await checkAndVerify(module.locator('input[name$="-softwareBillingOption"][value="payg"]'));
  await fillAndVerify(module.locator('input[name="managedStorageUnits"]'), item.storageUnits);
  await fillAndVerify(module.locator('input[name="additionalIopsSize"]'), '0');
  await fillAndVerify(module.locator('input[name="backupStorageSize"]'), '1');
  await fillAndVerify(module.locator('input[name="ltrDatabaseSize"]'), '0');
}

async function verifySqlMiItems(page) {
  const modules = page.locator('[data-testid="azure-sql-module"]');
  const count = await modules.count();
  if (count !== sqlMiItems.length) {
    throw new Error(`Expected ${sqlMiItems.length} SQL MI lines, found ${count}.`);
  }

  for (const [index, item] of sqlMiItems.entries()) {
    const module = modules.nth(index);
    await expectValue(module.locator('input[name="displayName"]'), item.name);
    await expectValue(module.locator('select[name="region"]'), 'sweden-central');
    await expectValue(module.locator('select[name="vcoreTier"]'), 'next-gen-general-purpose');
    await expectValue(module.locator('select[name="generation"]'), 'premium-series');
    await expectValue(module.locator('select[name="instanceSize"]'), item.vcores);
    await expectValue(module.locator('select[name="ramMemory"]'), item.memory);
    await expectValue(module.locator('select[name="recovery"]'), 'primaryinstance');
    await expectValue(module.locator('select[name="zoneRedundancy"]'), 'local');
    await expectValue(module.locator('input[name="managedCount"]'), '1');
    await expectValue(module.locator('input[name="hours"]'), '730');
    await expectChecked(module.locator('input[name$="-databaseBillingOption"][value="payg"]'));
    await expectChecked(module.locator('input[name$="-softwareBillingOption"][value="payg"]'));
    await expectValue(module.locator('input[name="managedStorageUnits"]'), item.storageUnits);
    await expectValue(module.locator('input[name="additionalIopsSize"]'), '0');
    await expectValue(module.locator('input[name="backupStorageSize"]'), '1');
    await expectValue(module.locator('input[name="ltrDatabaseSize"]'), '0');
  }
}

async function fillAndVerify(locator, value) {
  await locator.fill(value);
  await locator.press('Tab');
  await expectValue(locator, value);
}

async function selectAndVerify(locator, value) {
  await locator.selectOption(value);
  await expectValue(locator, value);
}

async function checkAndVerify(locator) {
  await locator.check();
  await expectChecked(locator);
}

async function expectValue(locator, expected) {
  const actual = await locator.inputValue();
  if (actual !== expected) {
    throw new Error(
      `Calculator control read-back mismatch: expected ${expected}, received ${actual}.`
    );
  }
}

async function expectChecked(locator) {
  if (!(await locator.isChecked())) {
    throw new Error('Calculator billing control did not remain selected.');
  }
}

async function findEdgeExecutable() {
  const candidates = [
    process.env.ProgramFiles,
    process.env['ProgramFiles(x86)'],
    process.env.LOCALAPPDATA
  ]
    .filter(Boolean)
    .map((root) => path.join(root, 'Microsoft', 'Edge', 'Application', 'msedge.exe'));

  for (const candidate of candidates) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Continue through the fixed Microsoft Edge installation locations.
    }
  }

  throw new Error('Microsoft Edge Stable was not found in an approved installation location.');
}

async function launchOrdinaryEdge(edgeExecutable, profileDirectory) {
  const child = spawn(
    edgeExecutable,
    [
      `--user-data-dir=${profileDirectory}`,
      '--new-window',
      '--no-first-run',
      '--no-default-browser-check',
      calculatorUrl
    ],
    {
      shell: false,
      stdio: 'ignore',
      windowsHide: false
    }
  );

  const [exitCode] = await once(child, 'exit');
  return exitCode;
}
