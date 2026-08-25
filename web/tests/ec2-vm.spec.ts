import { expect, test, type Page } from '@playwright/test';

async function mockApplication(page: Page): Promise<void> {
  await page.route('**/api/v1/session', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ mode: 'guest', display_name: null, privacy_consent: null })
    })
  );
  await page.route('**/api/v1/catalog/**', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ items: [] })
    })
  );
}

test('creates an EC2 Windows VM project without SQL inputs', async ({ page }) => {
  await mockApplication(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Create estimate' }).click();

  await expect(page.locator('.source-options button b')).toHaveText([
    'Amazon EC2',
    'EC2 Windows VM',
    'Amazon RDS',
    'On premises',
    'SQL Pay As You Go'
  ]);

  const vmOption = page.getByRole('button', { name: /EC2 Windows VM/ });
  await vmOption.click();
  await expect(vmOption).toHaveClass(/selected/);
  await expect(page.getByLabel('AWS region')).toBeVisible();
  await expect(page.getByLabel('Azure region')).toBeVisible();
  await page.getByRole('button', { name: 'Create project' }).click();

  await expect(page.getByRole('heading', { name: 'VM workloads' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Add VM', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Windows EC2 instance' })).toBeVisible();
  await expect(page.getByLabel('Instance type')).toHaveValue('r6id.8xlarge');
  await expect(page.getByLabel('Instance store use')).toHaveValue('not_used');
  await expect(page.getByLabel('Azure target override')).toHaveAttribute(
    'placeholder',
    'Automatic selection'
  );
  await expect(page.getByLabel('Role')).toHaveValue('os');
  await expect(page.getByLabel('Capacity (GB)')).toHaveValue('1024');

  await expect(page.getByLabel('SQL edition')).toHaveCount(0);
  await expect(page.getByLabel('License basis')).toHaveCount(0);
  await expect(page.getByLabel('SQL data / instance (GB)')).toHaveCount(0);
  await expect(page.getByText('Azure SQL MI pricing override', { exact: true })).toHaveCount(0);
});
