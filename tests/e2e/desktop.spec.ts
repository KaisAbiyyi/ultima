import { expect, test } from '@playwright/test';
test('NC-06 browser preview fails closed: no fake agent or network calls', async ({ page }) => {
  await page.goto('/?desktop=1');
  await expect(page.getByTestId('nc06-browser-limit')).toContainText('Browser preview only');
  await expect(page.getByRole('button', { name: 'Connect to Codex' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Send message to Codex' })).toBeDisabled();
  await expect(page.getByTestId('nc06-message-count')).toHaveText('0 messages');
});
test('NC-06 recent projects, keyboard navigation and theme', async ({ page }) => {
  await page.goto('/?desktop=1');
  await page.getByRole('textbox', { name: 'Absolute project folder' }).fill('C:\\projects\\ultima');
  await page.getByRole('button', { name: 'Add' }).click();
  await expect(page.getByRole('button', { name: 'U ultima' })).toBeVisible();
  await page.keyboard.press('Control+k');
  await expect(page.getByPlaceholder('Find a project or thread')).toBeFocused();
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await expect(page.getByRole('button', { name: 'U ultima' })).toBeVisible();
});
