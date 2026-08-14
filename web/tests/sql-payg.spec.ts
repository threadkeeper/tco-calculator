import { expect, test, type Page } from '@playwright/test';

async function mockGuestApplication(page: Page): Promise<void> {
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

test('calculates SQL PAYG discount without regions, resources, or price resolution', async ({
  page
}) => {
  await mockGuestApplication(page);
  let calculationRequest: Record<string, unknown> | null = null;
  let priceResolutionRequests = 0;

  await page.route('**/api/v1/pricing/**', (route) => {
    priceResolutionRequests += 1;
    return route.abort();
  });
  await page.route('**/api/v1/calculations', (route) => {
    calculationRequest = route.request().postDataJSON() as Record<string, unknown>;
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        formula_version: '1.0.0',
        aws_snapshot_id: null,
        azure_snapshot_id: null,
        resource_results: [],
        portfolio_totals: {
          aws_all_rows_total: '20000',
          portfolio_after_selected_parity: '40296',
          portfolio_difference: '-20296',
          comparable_resource_count: 1,
          no_mapping_resource_count: 0,
          price_unavailable_resource_count: 0
        },
        warnings: [],
        sql_payg_analysis: {
          enterprise_licensed_cores: 8,
          standard_licensed_cores: 16,
          software_assurance_annual_usd: '20000',
          annual_hours: 8760,
          enterprise_payg_usd_per_core_hour: '0.375',
          standard_payg_usd_per_core_hour: '0.100',
          payg_gross_annual_usd: '40296.000',
          required_payg_discount: '0.5036728211236847329759777645',
          payg_at_breakeven_usd: '20000',
          outcome: 'discount_required',
          rate_source_url: 'https://prices.azure.com/api/retail/prices',
          rate_verified_on: '2026-08-07'
        }
      })
    });
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Create estimate' }).click();
  await page.getByRole('button', { name: /SQL Pay As You Go/ }).click();
  await expect(page.getByLabel('AWS region')).toHaveCount(0);
  await expect(page.getByLabel('Azure region')).toHaveCount(0);
  await page.getByRole('button', { name: 'Create project' }).click();

  await expect(page.getByRole('heading', { name: 'SQL Pay As You Go' })).toBeVisible();
  await expect(page.getByText('Inventory', { exact: true })).toHaveCount(0);
  await expect(page.locator('main input')).toHaveCount(3);
  await page.getByLabel('SQL Enterprise licensed cores').fill('8');
  await page.getByLabel('SQL Standard licensed cores').fill('16');
  await page.getByLabel('Annual Software Assurance spend (USD)').fill('20000');
  await page.getByRole('button', { name: 'Calculate discount' }).click();

  await expect(page.getByRole('heading', { name: 'Required PAYG discount' })).toBeVisible();
  await expect(page.getByText('50.37%')).toBeVisible();
  await expect(page.getByText('$40,296.00')).toBeVisible();
  expect(priceResolutionRequests).toBe(0);
  expect(calculationRequest).toMatchObject({
    settings: {
      project_type: 'sql_payg',
      aws_region: null,
      azure_region: 'global',
      sql_payg: {
        enterprise_licensed_cores: 8,
        standard_licensed_cores: 16,
        software_assurance_annual_usd: '20000'
      }
    },
    resources: [],
    aws_price_snapshot_id: null,
    azure_price_snapshot_id: null
  });
});
