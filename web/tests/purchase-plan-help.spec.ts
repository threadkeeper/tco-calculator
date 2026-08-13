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

test('explains every SQL MI purchase plan and Azure Hybrid Benefit', async ({ page }) => {
  await mockApplication(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Create estimate' }).click();
  await page.getByRole('button', { name: 'Create project' }).click();

  const selector = page.getByRole('group', { name: 'Azure SQL MI pricing override' });
  const trigger = selector.getByRole('button', { name: /Compute commitment/ });
  const plans = [
    { label: 'Pay as you go', discount: 'Commitment discount: none' },
    { label: '1-year reserved', discount: 'up to 33% off compute' },
    { label: '3-year reserved', discount: 'up to 33% off compute' },
    { label: '1-year savings plan', discount: 'no single fixed discount percentage' }
  ];

  for (const plan of plans) {
    await trigger.click();
    await selector.getByRole('button', { name: `About ${plan.label}` }).click();

    const dialog = page.getByRole('dialog', { name: plan.label });
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText(plan.discount);
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(trigger).toBeFocused();
  }

  const ahbInfo = selector.getByRole('button', { name: 'About Azure Hybrid Benefit' });
  await ahbInfo.click();
  const ahbDialog = page.getByRole('dialog', { name: 'Azure Hybrid Benefit' });
  await expect(ahbDialog).toContainText('can save up to 55%');
  await expect(ahbDialog).toContainText('total savings can reach up to 82%');
  await ahbDialog.getByRole('button', { name: 'Got it' }).click();
  await expect(ahbInfo).toBeFocused();

  const bounds = await selector.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(await page.evaluate(() => innerWidth));
});
