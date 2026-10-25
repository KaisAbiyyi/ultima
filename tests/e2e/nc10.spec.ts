import { expect, test } from '@playwright/test';

test('NC-10 timing hooks stay inactive until opt-in and reset after stop/remount', async ({ page }) => {
  await page.goto('/?desktop=1');
  const snapshot = () => page.evaluate(() => (window as any).__ultimaPerformance.snapshot());
  await page.waitForFunction(() => !!(window as any).__ultimaPerformance);
  expect(await snapshot()).toMatchObject({ enabled: false, input: [], stream: [] });
  const composer = page.getByRole('textbox', { name: 'Message to Codex' });
  // A synthetic dispatch checks hook wiring only. This is not native keyboard
  // latency or evidence that the disabled browser composer can run the engine.
  await composer.dispatchEvent('input');
  expect(await snapshot()).toMatchObject({ enabled: false, input: [] });
  await page.evaluate(() => (window as any).__ultimaPerformance.start());
  await composer.dispatchEvent('input');
  await expect.poll(async () => (await snapshot()).input.length).toBe(1);
  expect(await snapshot()).toMatchObject({ boundary: 'flush-two-raf-proxy', pending: 0 });
  await page.evaluate(() => (window as any).__ultimaPerformance.stop());
  expect(await snapshot()).toMatchObject({ enabled: false, input: [], stream: [] });
  await page.reload();
  await page.waitForFunction(() => !!(window as any).__ultimaPerformance);
  expect(await snapshot()).toMatchObject({ enabled: false, input: [], stream: [] });
});
