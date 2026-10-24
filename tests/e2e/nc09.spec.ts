import { expect, test } from '@playwright/test';

test('NC-09 desktop search restores keyboard focus and exposes active project', async ({ page }) => {
  await page.goto('/?desktop=1');
  await expect(page.getByTestId('nc06-browser-limit')).toBeVisible();
  const executable = page.locator('#nc06-executable');
  await executable.focus();
  await page.keyboard.press('Control+k');
  await expect(page.getByRole('searchbox', { name: 'Search projects and threads' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(executable).toBeFocused();
  await page.getByRole('textbox', { name: 'Absolute project folder' }).fill('C:\\projects\\nc09');
  await page.getByRole('button', { name: 'Add project' }).click();
  await expect(page.getByRole('button', { name: 'N nc09' })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('button', { name: 'Toggle theme' })).toHaveAttribute('aria-pressed', 'false');
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.getByRole('button', { name: 'Toggle theme' })).toHaveAttribute('aria-pressed', 'true');
});

test('NC-09 mobile navigation is inert when closed, keyboard accessible when open', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/?desktop=1');
  await expect(page.getByTestId('nc06-browser-limit')).toBeVisible();
  const menu = page.getByRole('button', { name: 'Open navigation' });
  const sidebar = page.locator('#nc09-sidebar');
  await expect(sidebar).toHaveAttribute('inert', '');
  await menu.click();
  await expect(menu).toHaveAttribute('aria-expanded', 'true');
  await expect(sidebar).not.toHaveAttribute('inert', '');
  await expect(page.getByRole('searchbox', { name: 'Search projects and threads' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveAttribute('aria-expanded', 'false');
  await expect(sidebar).toHaveAttribute('inert', '');
  await expect(menu).toBeFocused();
});

test('NC-09 project Enter does not commit while Windows IME is composing', async ({ page }) => {
  await page.goto('/?desktop=1');
  await expect(page.getByTestId('nc06-browser-limit')).toBeVisible();
  const folder = page.getByRole('textbox', { name: 'Absolute project folder' });
  await folder.fill('C:\\projects\\imefixture');
  await folder.dispatchEvent('compositionstart');
  await folder.dispatchEvent('keydown', { key: 'Enter', bubbles: true, isComposing: true, keyCode: 229 });
  await expect(page.getByRole('button', { name: 'I imefixture' })).toHaveCount(0);
  await folder.dispatchEvent('compositionend');
  await page.getByRole('button', { name: 'Add project' }).click();
  await expect(page.getByRole('button', { name: 'I imefixture' })).toBeVisible();
});

test('NC-09 screenshot matrix: both themes at 100%/200% DPI and mobile', async ({ browser }, testInfo) => {
  for (const [width, height, scale] of [[1440, 900, 1], [1440, 900, 2], [390, 844, 2]]) {
    const context = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: scale, reducedMotion: 'reduce' });
    try {
      const page = await context.newPage();
      await page.goto('/?desktop=1');
      await expect(page.getByTestId('nc06-browser-limit')).toBeVisible();
      for (const theme of ['dark', 'light']) {
        if (theme === 'light') await page.getByRole('button', { name: 'Toggle theme' }).click();
        await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
        await page.screenshot({ path: testInfo.outputPath(`nc09-${width}x${height}-${scale}x-${theme}.png`), fullPage: true, animations: 'disabled' });
      }
    } finally { await context.close(); }
  }
});
