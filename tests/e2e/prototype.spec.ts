import { expect, test } from '@playwright/test';

test('virtualizes 10k messages without growing the DOM', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('message-count')).toHaveText('10,000 messages');
  await expect(page.getByTestId('message-row').first()).toBeVisible();
  expect(await page.getByTestId('message-row').count()).toBeLessThan(40);
  await page.getByTestId('message-feed').evaluate((element) => { element.scrollTop = 0; });
  await expect(page.getByTestId('message-row').first()).toContainText('Review the memory footprint');
  expect(await page.getByTestId('message-row').count()).toBeLessThan(40);
});

test('keyboard-first navigation, composition and theme switch', async ({ page }) => {
  await page.goto('/');
  await page.keyboard.press('Control+k');
  await expect(page.getByPlaceholder('Find a project')).toBeFocused();
  await page.keyboard.press('Control+l');
  await expect(page.locator('#composer')).toBeFocused();
  await page.keyboard.press('Control+n');
  await expect(page.getByTestId('message-count')).toHaveText('0 messages');
  await page.locator('#composer').fill('Test prompt');
  await page.locator('#composer').press('Enter');
  await expect(page.getByTestId('message-count')).toHaveText('2 messages');
  await expect(page.getByText('This is a simulated Codex response', { exact: false })).toBeVisible({ timeout: 10_000 });
  await page.getByRole('button', { name: 'Switch to light theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('desktop light and dark visual artifacts', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/');
  await page.screenshot({ path: testInfo.outputPath('ultima-desktop-dark.png'), fullPage: true });
  await page.getByRole('button', { name: 'Switch to light theme' }).click();
  await page.screenshot({ path: testInfo.outputPath('ultima-desktop-light.png'), fullPage: true });
});

test('mobile navigation and capture', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Open navigation' }).click();
  await expect(page.getByLabel('Primary navigation')).toHaveClass(/open/);
  await page.screenshot({ path: testInfo.outputPath('ultima-mobile-navigation.png'), fullPage: true });
  await page.getByRole('button', { name: 'Close navigation' }).click();
  await expect(page.getByLabel('Primary navigation')).not.toHaveClass(/open/);
});
