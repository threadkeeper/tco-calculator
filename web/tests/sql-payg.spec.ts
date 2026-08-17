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

test('calculates SQL PAYG savings or overage from monthly hours and an applied discount', async ({
  page
}) => {
  await mockGuestApplication(page);
  const calculationRequests: Record<string, unknown>[] = [];
  let priceResolutionRequests = 0;

  await page.route('**/api/v1/pricing/**', (route) => {
    priceResolutionRequests += 1;
    return route.abort();
  });
  await page.route('**/api/v1/calculations', (route) => {
    calculationRequests.push(route.request().postDataJSON() as Record<string, unknown>);
    const isOverage = calculationRequests.length > 1;
    const sourceTotal = isOverage ? '5000' : '20000';
    const requiredDiscount = isOverage ? '0.4338768115942028985507246377' : '0';
    const savings = isOverage ? '-1624.00000' : '13376.00000';
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        formula_version: '1.1.0',
        aws_snapshot_id: null,
        azure_snapshot_id: null,
        resource_results: [],
        portfolio_totals: {
          aws_all_rows_total: sourceTotal,
          aws_mapped_rows_total: sourceTotal,
          azure_mapped_rows_total: '8832.000',
          required_portfolio_adjustment: requiredDiscount,
          selected_parity_adjustment: '0.25',
          portfolio_after_selected_parity: '6624.00000',
          portfolio_difference: isOverage ? '1624.00000' : '-13376.00000',
          comparable_resource_count: 1,
          no_mapping_resource_count: 0,
          price_unavailable_resource_count: 0
        },
        warnings: [],
        sql_payg_analysis: {
          enterprise_licensed_cores: 8,
          standard_licensed_cores: 16,
          software_assurance_annual_usd: sourceTotal,
          annual_hours: '1920',
          enterprise_payg_usd_per_core_hour: '0.375',
          standard_payg_usd_per_core_hour: '0.100',
          payg_gross_annual_usd: '8832.000',
          required_payg_discount: requiredDiscount,
          payg_at_breakeven_usd: isOverage ? '5000' : '8832.000',
          applied_payg_discount: '0.25',
          payg_net_annual_usd: '6624.00000',
          annual_savings_usd: savings,
          outcome: isOverage ? 'discount_required' : 'no_discount_needed',
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
  await expect(page.locator('main input')).toHaveCount(5);
  await page.getByLabel('SQL Enterprise licensed cores').fill('8');
  await page.getByLabel('SQL Standard licensed cores').fill('16');
  await page.getByLabel('Annual Software Assurance spend (USD)').fill('20000');
  await page.getByRole('button', { name: 'Monthly' }).click();
  await page.getByLabel('Hours per month').fill('160');
  await page.getByLabel('Applied PAYG discount (%)').fill('25');
  await page.getByRole('button', { name: 'Calculate savings' }).click();

  await expect(page.getByRole('heading', { name: 'Annual savings' })).toBeVisible();
  await expect(page.getByText('+$13,376.00')).toBeVisible();
  await expect(page.getByText('$6,624.00')).toBeVisible();
  expect(priceResolutionRequests).toBe(0);
  expect(calculationRequests[0]).toMatchObject({
    settings: {
      project_type: 'sql_payg',
      aws_region: null,
      azure_region: 'global',
      default_annual_hours: '1920',
      selected_parity_adjustment: '0.25',
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

  await page.getByLabel('Annual Software Assurance spend (USD)').fill('5000');
  await page.getByRole('button', { name: 'Calculate savings' }).click();

  await expect(page.getByRole('heading', { name: 'Annual overage' })).toBeVisible();
  await expect(page.getByText('$1,624.00')).toBeVisible();
  await expect(page.getByText('43.39%')).toBeVisible();
  expect(calculationRequests[1]).toMatchObject({
    settings: {
      default_annual_hours: '1920',
      selected_parity_adjustment: '0.25',
      sql_payg: {
        software_assurance_annual_usd: '5000'
      }
    }
  });
});
