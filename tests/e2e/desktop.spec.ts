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

// Synthetic browser bridge only; never starts Codex or claims native acceptance.
async function approvalPreview(page: import('@playwright/test').Page) {
  await page.route('**/src/lib/desktop-bridge.ts*', route => route.fulfill({
    contentType: 'application/javascript',
    body: `
      let listener, id = 0;
      window.__approvalCalls = [];
      window.__emitApproval = event => listener({session:1, ...event});
      export const desktopAvailable = () => true;
      export const nativeBridge = async () => ({
        listen: async fn => { listener = fn; return () => {}; },
        connect: async () => { setTimeout(() => listener({session:1,type:'status',state:'ready'}),0); return 1; },
        call: async method => {
          const current = ++id;
          setTimeout(() => {
            listener({session:1,type:'reply',id:current,method,ok:true,
              result:method === 'thread/start' ? {thread:{id:'t'}} : {data:[]}});
            if (method === 'turn/start') {
              listener({session:1,type:'turnStarted',params:{threadId:'t',turn:{id:'u'}}});
              window.__fixtureTurnReady = true;
            }
          },0);
          return current;
        },
        approval: async (...args) => { window.__approvalCalls.push(args); },
        disconnect: async () => listener({session:1,type:'status',state:'disconnected'}),
        runtimeStatus: async () => ({mode:'system',version:null,managedSupported:false,pinnedVersion:null,assetUrl:null})
      });`
  }));
  await page.goto('/?desktop=1');
  await page.getByRole('textbox', { name:'Absolute project folder' }).fill('/project');
  await page.getByRole('button', { name:'Add project' }).click();
  await page.locator('#nc06-executable').fill('/synthetic/codex');
  await page.getByRole('button', { name:'Connect to Codex' }).click();
  await expect(page.getByRole('textbox', { name:'Message to Codex', exact:true })).toBeEnabled();
  await page.getByRole('textbox', { name:'Message to Codex', exact:true }).fill('Synthetic approval preview');
  await page.getByRole('button', { name:'Send message to Codex' }).click();
  await page.waitForFunction(() => (window as any).__fixtureTurnReady === true);
  await expect(page.getByRole('button', { name:'Interrupt Codex turn' })).toBeVisible();
}

import { readFileSync } from 'node:fs';
const localFixture = JSON.parse(readFileSync(new URL('../fixtures/nc10g-local-approval.json', import.meta.url), 'utf8'));
const localRequest = { id:localFixture.id, ...localFixture.params, request:localFixture.params };
async function emitApproval(page: import('@playwright/test').Page, request: unknown = localRequest) {
  await page.evaluate(request => {
    const emit = (window as any).__emitApproval;
    emit({type:'itemStarted',params:{threadId:'t',turnId:'u',item:{id:'i',type:'commandExecution',command:'echo safe',cwd:'/project'}}});
    emit({type:'approvalPending',request});
  }, request);
}

test('NC-10G local environment review sends one-shot accept with the complete descriptor', async ({ page }) => {
  await approvalPreview(page);
  await emitApproval(page);
  await expect(page.getByTestId('nc10e-request')).toContainText('"environmentId": "local"');
  const allow = page.getByRole('button', { name:'Allow once' });
  await expect(allow).toBeDisabled();
  await page.getByRole('checkbox', { name:'I reviewed this request, permission scope and matching evidence' }).check();
  await expect(allow).toBeEnabled();
  await allow.click();
  await expect.poll(() => page.evaluate(() => (window as any).__approvalCalls.length)).toBe(1);
  const [session, request, decision] = await page.evaluate(() => (window as any).__approvalCalls[0]);
  expect(session).toBe(1);
  expect(request.request).toEqual(localFixture.params);
  expect(decision).toBe('accept');
  await expect(allow).toHaveCount(0);
});

for (const identity of ['remote','null','missing']) {
  test(`NC-10G ${identity} replacement after review shuts down without dispatching accept`, async ({ page }) => {
    await approvalPreview(page);
    await emitApproval(page);
    await page.getByRole('checkbox', { name:'I reviewed this request, permission scope and matching evidence' }).check();
    await expect(page.getByRole('button', { name:'Allow once' })).toBeEnabled();
    const request: Record<string,unknown> = structuredClone(localFixture.params);
    if (identity === 'missing') delete request.environmentId;
    else request.environmentId = identity === 'null' ? null : identity;
    await page.evaluate(request => (window as any).__emitApproval({type:'approvalPending', request}),
      { ...localRequest, request });
    await expect(page.getByRole('alert')).toContainText('Conflicting approval descriptor');
    await expect(page.getByRole('button', { name:'Allow once' })).toHaveCount(0);
    expect(await page.evaluate(() => (window as any).__approvalCalls)).toEqual([]);
  });
}

for (const identity of ['remote','custom','null','missing']) {
  test(`NC-10G original ${identity} identity remains inspectable and decline-only`, async ({ page }) => {
    await approvalPreview(page);
    const request: Record<string,unknown> = structuredClone(localFixture.params);
    if (identity === 'missing') delete request.environmentId;
    else request.environmentId = identity === 'null' ? null : identity;
    await emitApproval(page,{ ...localRequest,request });
    await expect(page.getByTestId('nc10e-request')).toBeVisible();
    await expect(page.getByTestId('nc07-approval')).toContainText('Execution environment is unsupported or unidentified.');
    await expect(page.getByRole('button', { name:'Allow once' })).toBeDisabled();
    await expect(page.getByRole('checkbox', { name:'I reviewed this request, permission scope and matching evidence' })).toBeDisabled();
    await expect(page.getByRole('button', { name:'Decline', exact:true })).toBeEnabled();
    await page.getByRole('button', { name:'Decline', exact:true }).click();
    const [,sent,decision] = await page.evaluate(() => (window as any).__approvalCalls[0]);
    expect(sent.request).toEqual(request);
    expect(decision).toBe('decline');
  });
}

const fileFixture=JSON.parse(readFileSync(new URL('../fixtures/nc10g-filechange-approval.json',import.meta.url),'utf8'));
for (const identity of ['missing','null','local']) {
  test(`NC-10G FileChange ${identity} environment metadata preserves the absence-only contract`,async ({page})=>{
    await approvalPreview(page);
    const request: Record<string,unknown>=structuredClone(fileFixture.params);
    if (identity!=='missing') request.environmentId=identity==='null' ? null : identity;
    await page.evaluate(request=>{
      const emit=(window as any).__emitApproval;
      emit({type:'itemStarted',params:{threadId:'t',turnId:'u',item:{id:'i',type:'fileChange',changes:[{path:'safe.txt',diff:'+safe change'}]}}});
      emit({type:'approvalPending',request});
    },{id:fileFixture.id,kind:'fileChange',...fileFixture.params,request});
    const card=page.getByTestId('nc07-approval');
    await expect(card).toBeVisible();
    await expect(card.getByRole('button',{name:'Decline',exact:true})).toBeEnabled();
    await expect(card.getByRole('button',{name:'Allow once'})).toBeDisabled();
    await card.getByRole('button',{name:'Review file diff',exact:true}).click();
    if (identity==='missing') {
      await card.getByRole('checkbox').check();
      await expect(card.getByRole('button',{name:'Allow once'})).toBeEnabled();
      await card.getByRole('button',{name:'Allow once'}).click();
    } else {
      await expect(card).toContainText('File-change environment metadata is unsupported.');
      await expect(card.getByRole('checkbox')).toBeDisabled();
      await card.getByRole('button',{name:'Decline',exact:true}).click();
    }
    const [,sent,decision]=await page.evaluate(()=>(window as any).__approvalCalls[0]);
    expect(sent.request).toEqual(request);
    expect(decision).toBe(identity==='missing' ? 'accept' : 'decline');
  });
}
