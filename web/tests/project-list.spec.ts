import { expect, test } from '@playwright/test';

test('shows signed Azure savings with positive and negative tints', async ({ page }) => {
  await page.route('**/api/v1/session', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        mode: 'authenticated',
        display_name: 'Test user',
        email_address: null,
        privacy_consent: {
          required: false,
          notice_version: '2026-08-01',
          accepted_at: '2026-08-01T12:00:00Z',
          allow_contact: false,
          email_address: null
        }
      })
    })
  );
  await page.route('**/api/v1/catalog/**/regions', (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: '{"items":[]}' })
  );
  await page.route('**/api/v1/projects', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          id: '11111111-1111-4111-8111-111111111111',
          name: 'Azure is cheaper',
          project_type: 'ec2',
          modified_at: '2026-08-13T12:00:00Z',
          source_region: 'eastus',
          azure_region: 'eastus2',
          resource_count: 2,
          source_annual_total: '125000.00',
          azure_annual_total: '100000.00',
          azure_savings: '25000.00'
        },
        {
          id: '22222222-2222-4222-8222-222222222222',
          name: 'Azure is costlier',
          project_type: 'rds',
          modified_at: '2026-08-13T12:00:00Z',
          source_region: 'westus',
          azure_region: 'westus2',
          resource_count: 1,
          source_annual_total: '80000.00',
          azure_annual_total: '92500.00',
          azure_savings: '-12500.00'
        }
      ])
    })
  );

  await page.goto('/');

  await expect(page.getByText('Azure Savings', { exact: true })).toBeVisible();
  const positive = page.locator('.azure-savings[data-tone="positive"]');
  const negative = page.locator('.azure-savings[data-tone="negative"]');
  await expect(positive).toHaveText('+$25,000.00');
  await expect(positive).toHaveCSS('color', 'rgb(100, 201, 148)');
  await expect(positive).toHaveCSS('background-color', 'rgb(25, 54, 41)');
  await expect(negative).toHaveText('-$12,500.00');
  await expect(negative).toHaveCSS('color', 'rgb(242, 170, 164)');
  await expect(negative).toHaveCSS('background-color', 'rgb(58, 35, 33)');
});
