import { expect, test, type Page } from '@playwright/test';

type ProjectRequest = {
  settings?: Record<string, unknown>;
  resources: Array<Record<string, unknown>>;
};

const awsSnapshotId = `aws-${'a'.repeat(64)}`;
const azureSnapshotId = `azure-${'b'.repeat(64)}`;

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

function requestPayload(pageRequest: { postDataJSON(): unknown }): ProjectRequest {
  return pageRequest.postDataJSON() as ProjectRequest;
}

function expectEc2ResourceContract(payload: ProjectRequest): void {
  const resource = payload.resources[0];
  expect(resource.server_name).toBe('sql-prod-01');
  expect(resource.quantity).toBe(2);
  expect(resource.annual_hours_per_instance).toBe('8000.5');
  expect(resource.source_ram_gb_per_instance).toBe('384.5');
  expect(resource.sql_data_gb_per_instance).toBe('1');

  const volumes = resource.volumes as Array<Record<string, unknown>>;
  expect(volumes[0]).toMatchObject({
    capacity_gb: '2048.5',
    provisioned_iops: 6000,
    throughput_mibps: '250.25'
  });
}

test('normalizes edited inputs while preserving EC2 SQL data and EBS capacity', async ({
  page
}) => {
  await mockApplication(page);
  const priceRequests: ProjectRequest[] = [];
  let calculationRequest: ProjectRequest | null = null;

  await page.route('**/api/v1/pricing/*/resolve', (route) => {
    const provider = new URL(route.request().url()).pathname.includes('/aws/') ? 'aws' : 'azure';
    priceRequests.push(requestPayload(route.request()));
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        provider,
        status: 'cached',
        snapshot_id: provider === 'aws' ? awsSnapshotId : azureSnapshotId,
        retrieved_at: '2026-08-11T12:00:00Z',
        warnings: []
      })
    });
  });
  await page.route('**/api/v1/calculations', (route) => {
    calculationRequest = requestPayload(route.request());
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        formula_version: '1.0.0',
        portfolio_totals: {
          aws_all_rows_total: '0',
          portfolio_after_selected_parity: '0',
          portfolio_difference: '0',
          comparable_resource_count: 0,
          no_mapping_resource_count: 0,
          price_unavailable_resource_count: 1
        },
        resource_results: [],
        warnings: []
      })
    });
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Create estimate' }).click();
  await page.getByRole('button', { name: 'Create project' }).click();

  await page.getByText('Project settings', { exact: true }).click();
  await page.getByLabel('Source compute discount').fill('0.125');
  await page.getByLabel('Server name').fill('  sql-prod-01  ');
  await page.getByLabel('Quantity').fill('2');
  await page.getByLabel('Annual hours / instance').fill('8000.5');
  await page.getByLabel('Source RAM / instance (GiB)').fill('384.5');
  await page.getByLabel('SQL data / instance (GB)').fill('1');
  await page.locator('.volume-row select').first().selectOption('gp3');
  await page.getByLabel('Capacity (GB)').fill('2048.5');
  await expect(page.getByLabel('SQL data / instance (GB)')).toHaveValue('1');
  await page.getByLabel('Provisioned IOPS').fill('6000');
  await page.getByLabel('Throughput (MiB/s)').fill('250.25');
  await page.getByRole('button', { name: 'Calculate estimate' }).click();

  await expect(page.getByRole('heading', { name: 'Annual comparison' })).toBeVisible();
  expect(priceRequests).toHaveLength(2);
  for (const payload of priceRequests) expectEc2ResourceContract(payload);
  expect(calculationRequest).not.toBeNull();
  expectEc2ResourceContract(calculationRequest!);
  expect(calculationRequest!.settings).toMatchObject({
    source_compute_discount: '0.125',
    source_license_discount: '0',
    source_storage_discount: '0',
    azure_compute_discount: '0',
    azure_license_discount: '0',
    azure_storage_discount: '0',
    selected_parity_adjustment: '0',
    default_annual_hours: '8760'
  });
});
