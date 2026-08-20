import { access, mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

import { chromium } from '@playwright/test';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const fixtureRoot = path.resolve(scriptDirectory, '../../tests/assistant-workload-classification');
const manifestPath = path.join(fixtureRoot, 'cases.json');
const resetResults = process.argv.includes('--reset-results');
const allowedFamilies = new Set(['ec2', 'rds', 'on_prem', 'sql_payg']);

const cases = JSON.parse(await readFile(manifestPath, 'utf8'));
validateManifest(cases);

const browser = await chromium.launch({
  headless: true,
  channel: process.platform === 'win32' ? 'msedge' : undefined
});
try {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
    deviceScaleFactor: 1,
    colorScheme: 'light',
    reducedMotion: 'reduce'
  });
  const page = await context.newPage();

  for (const fixture of cases) {
    const caseDirectory = path.join(fixtureRoot, fixture.id);
    await mkdir(caseDirectory, { recursive: true });

    const html = renderFixture(fixture);
    const htmlPath = path.join(caseDirectory, 'fixture.html');
    await writeFile(htmlPath, html, 'utf8');
    await writeFile(
      path.join(caseDirectory, 'expected.json'),
      `${JSON.stringify(fixture.expected, null, 2)}\n`,
      'utf8'
    );
    await writeInitialResult(caseDirectory, fixture);

    await page.goto(pathToFileURL(htmlPath).href, { waitUntil: 'load' });
    await page.screenshot({
      path: path.join(caseDirectory, 'input.png'),
      type: 'png',
      fullPage: false,
      animations: 'disabled'
    });
  }
} finally {
  await browser.close();
}

console.log(`Rendered ${cases.length} assistant classification fixture(s) in ${fixtureRoot}`);

function validateManifest(fixtures) {
  if (!Array.isArray(fixtures) || fixtures.length === 0) {
    throw new Error('cases.json must contain at least one fixture');
  }

  const identifiers = new Set();
  for (const fixture of fixtures) {
    if (
      typeof fixture.id !== 'string' ||
      !/^(ec2|rds|on_prem|sql_payg)\/[a-z0-9-]+$/.test(fixture.id) ||
      identifiers.has(fixture.id)
    ) {
      throw new Error(`Invalid or duplicate fixture id: ${fixture.id}`);
    }
    identifiers.add(fixture.id);
    if (!allowedFamilies.has(fixture.family) || !fixture.id.startsWith(`${fixture.family}/`)) {
      throw new Error(`Fixture family does not match its path: ${fixture.id}`);
    }
    if (!Array.isArray(fixture.sections) || fixture.sections.length === 0) {
      throw new Error(`Fixture must contain visible sections: ${fixture.id}`);
    }
    if (fixture.expected?.case_id !== fixture.id) {
      throw new Error(`Expected contract does not identify fixture: ${fixture.id}`);
    }
  }
}

async function writeInitialResult(caseDirectory, fixture) {
  const resultPath = path.join(caseDirectory, 'result.md');
  if (!resetResults) {
    try {
      await access(resultPath);
      return;
    } catch {
      // A missing result receives a stable not-run marker.
    }
  }

  const content = `# Live Foundry Evaluation Result

- Status: not run
- Case: \`${fixture.id}\`
- Evaluation identity: \`not_run\`
- Expected project type: \`${fixture.expected.project_type}\`

Run the opt-in live evaluator after local fixture and code validation. This file contains no generated draft yet.
`;
  await writeFile(resultPath, content, 'utf8');
}

function renderFixture(fixture) {
  const sections = fixture.sections.map(renderSection).join('\n');
  const badges = fixture.badges
    .map((badge) => `<span class="badge">${escapeHtml(badge)}</span>`)
    .join('');

  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${escapeHtml(fixture.title)}</title>
    <style>
      :root {
        --ink: #17212b;
        --muted: #5f6b76;
        --paper: #ffffff;
        --canvas: #edf0f2;
        --line: #cbd2d8;
        --accent: ${escapeHtml(fixture.accent)};
        --accent-soft: color-mix(in srgb, var(--accent) 12%, white);
      }
      * { box-sizing: border-box; }
      html, body { width: 100%; height: 100%; margin: 0; }
      body {
        color: var(--ink);
        background:
          linear-gradient(90deg, rgb(23 33 43 / 4%) 1px, transparent 1px) 0 0 / 24px 24px,
          linear-gradient(rgb(23 33 43 / 4%) 1px, transparent 1px) 0 0 / 24px 24px,
          var(--canvas);
        font-family: "Aptos", "Segoe UI", sans-serif;
        letter-spacing: 0;
      }
      main {
        width: 1320px;
        min-height: 900px;
        margin: 50px auto;
        background: var(--paper);
        border: 1px solid #b9c1c8;
        box-shadow: 0 18px 48px rgb(23 33 43 / 16%);
      }
      header {
        position: relative;
        padding: 40px 48px 34px;
        border-top: 10px solid var(--accent);
        border-bottom: 1px solid var(--line);
      }
      .kicker {
        color: var(--accent);
        font-size: 14px;
        font-weight: 700;
        text-transform: uppercase;
      }
      h1 {
        max-width: 920px;
        margin: 10px 0 8px;
        font-family: Georgia, "Times New Roman", serif;
        font-size: 38px;
        font-weight: 600;
        line-height: 1.12;
      }
      .subtitle { margin: 0; color: var(--muted); font-size: 18px; }
      .badges { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 22px; }
      .badge {
        padding: 7px 10px;
        border: 1px solid color-mix(in srgb, var(--accent) 36%, white);
        border-radius: 4px;
        background: var(--accent-soft);
        color: #27313a;
        font-size: 14px;
        font-weight: 650;
      }
      .case-id {
        position: absolute;
        top: 42px;
        right: 48px;
        color: var(--muted);
        font-family: Consolas, monospace;
        font-size: 13px;
      }
      .content {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 22px;
        padding: 30px 48px 38px;
      }
      section {
        min-width: 0;
        border-top: 3px solid var(--accent);
        background: #fff;
      }
      section.wide { grid-column: 1 / -1; }
      h2 { margin: 14px 0 12px; font-size: 18px; line-height: 1.25; }
      table { width: 100%; border-collapse: collapse; table-layout: fixed; font-size: 15px; }
      th, td {
        padding: 11px 12px;
        border: 1px solid var(--line);
        text-align: left;
        vertical-align: top;
        overflow-wrap: anywhere;
      }
      th { background: #f1f3f5; color: #35414c; font-size: 13px; font-weight: 700; }
      tbody tr:nth-child(even) td { background: #fafbfc; }
      dl {
        display: grid;
        grid-template-columns: minmax(160px, 42%) minmax(0, 1fr);
        margin: 0;
        border: 1px solid var(--line);
        border-bottom: 0;
      }
      dt, dd { margin: 0; padding: 10px 12px; border-bottom: 1px solid var(--line); }
      dt { color: var(--muted); background: #f7f8f9; font-size: 13px; font-weight: 700; }
      dd { font-size: 15px; font-weight: 600; overflow-wrap: anywhere; }
      .notes { margin: 0; padding: 14px 18px 14px 34px; border: 1px solid var(--line); }
      .notes li { margin: 5px 0; line-height: 1.35; }
      footer {
        display: flex;
        justify-content: space-between;
        gap: 24px;
        padding: 16px 48px;
        border-top: 1px solid var(--line);
        color: var(--muted);
        font-size: 12px;
      }
    </style>
  </head>
  <body>
    <main>
      <header>
        <div class="kicker">${escapeHtml(fixture.kicker)}</div>
        <div class="case-id">${escapeHtml(fixture.id)}</div>
        <h1>${escapeHtml(fixture.title)}</h1>
        <p class="subtitle">${escapeHtml(fixture.subtitle)}</p>
        <div class="badges">${badges}</div>
      </header>
      <div class="content">${sections}</div>
      <footer>
        <span>Synthetic classification fixture. No customer or production data.</span>
        <span>Values are test inputs, not prices or licensing advice.</span>
      </footer>
    </main>
  </body>
</html>
`;
}

function renderSection(section) {
  const className = section.wide ? ' class="wide"' : '';
  if (section.kind === 'facts') {
    const facts = section.items
      .map(([label, value]) => `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value)}</dd>`)
      .join('');
    return `<section${className}><h2>${escapeHtml(section.title)}</h2><dl>${facts}</dl></section>`;
  }
  if (section.kind === 'table') {
    const header = section.columns.map((column) => `<th>${escapeHtml(column)}</th>`).join('');
    const rows = section.rows
      .map((row) => `<tr>${row.map((cell) => `<td>${escapeHtml(cell)}</td>`).join('')}</tr>`)
      .join('');
    return `<section${className}><h2>${escapeHtml(section.title)}</h2><table><thead><tr>${header}</tr></thead><tbody>${rows}</tbody></table></section>`;
  }
  if (section.kind === 'notes') {
    const notes = section.items.map((note) => `<li>${escapeHtml(note)}</li>`).join('');
    return `<section${className}><h2>${escapeHtml(section.title)}</h2><ul class="notes">${notes}</ul></section>`;
  }
  throw new Error(`Unknown section kind: ${section.kind}`);
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}
