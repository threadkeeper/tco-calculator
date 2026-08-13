import { expect, test, type Page } from '@playwright/test';

const helpResponse = {
  answer: 'Azure region selects the public Azure SQL Managed Instance prices.',
  references: [{ control_id: 'project.azure-region', label: 'Azure region' }]
};

async function mockApplication(page: Page): Promise<void> {
  await page.route('**/api/v1/session', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ mode: 'guest', display_name: null, privacy_consent: null })
    })
  );
  await page.route('**/api/v1/catalog/**/regions', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: [] })
    })
  );
}

test.beforeEach(async ({ page }) => {
  await mockApplication(page);
});

test('opens, sends plain-text help, and returns focus after Escape', async ({ page }) => {
  await page.route('**/api/v1/assistant/help', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(helpResponse)
    })
  );
  await page.goto('/');

  const launcher = page.getByRole('button', { name: 'Open TCO assistant' });
  await expect(launcher).toBeVisible();
  await expect(launcher.locator('svg[data-icon="copilot"]')).toBeVisible();
  await expect(
    page
      .getByRole('link', { name: 'View Azure SQL TCO repository on GitHub' })
      .locator('svg[data-icon="github"]')
  ).toBeVisible();
  await launcher.click();

  const composer = page.getByLabel('Ask the TCO assistant');
  await expect(composer).toBeFocused();
  await composer.fill('What does Azure region mean?');
  await composer.press('Enter');

  await expect(composer).toHaveValue('');
  await expect(page.getByText(helpResponse.answer, { exact: true })).toBeVisible();
  await expect(page.getByText('Azure region', { exact: true })).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'TCO assistant' })).toBeHidden();
  await expect(launcher).toBeFocused();
});

test('cancels a pending response and stays within the viewport', async ({ page }) => {
  let releaseResponse!: () => void;
  const responseGate = new Promise<void>((resolve) => {
    releaseResponse = resolve;
  });
  await page.route('**/api/v1/assistant/help', async (route) => {
    await responseGate;
    await route.fulfill({ status: 504, body: '' }).catch(() => undefined);
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Open TCO assistant' }).click();

  const composer = page.getByLabel('Ask the TCO assistant');
  await composer.fill('Explain the calculation');
  await composer.press('Enter');
  await page.getByRole('button', { name: 'Cancel response' }).click();

  await expect(page.getByText('The response was cancelled.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Send question' })).toBeVisible();

  const panel = page.getByRole('dialog', { name: 'TCO assistant' });
  const box = await panel.boundingBox();
  const viewport = page.viewportSize();
  expect(box).not.toBeNull();
  expect(viewport).not.toBeNull();
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.y).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(viewport!.width);
  expect(box!.y + box!.height).toBeLessThanOrEqual(viewport!.height);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
    viewport!.width
  );
  releaseResponse();
});
