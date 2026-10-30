import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';
import cases from '../fixtures/nc10g-environments.json';
import fixture from '../fixtures/nc10g-local-approval.json';
import { canApprove, parseApproval, trackTool } from '../../src/lib/tool-events';

it('compares actual native→broker, host→broker and Svelte eligibility for every original identity', () => {
  const receiptPath = process.env.NC10G_PARITY_RECEIPT;
  if (!receiptPath) throw Error('NC10G_PARITY_RECEIPT must name the actual Rust parity receipt');
  const receipts = JSON.parse(readFileSync(receiptPath, 'utf8'));
  expect(receipts).toHaveLength(cases.length);
  expect(new Set(receipts.map((r: {name:string}) => r.name)).size).toBe(cases.length);
  const evidence = trackTool([], 'itemStarted', {threadId:'t',turnId:'u',
    item:{id:'i',type:'commandExecution',command:'echo safe',cwd:'/project'}});
  for (const testCase of cases) {
    const request: Record<string, unknown> = structuredClone(fixture.params);
    if (testCase.present) request.environmentId = testCase.value;
    else delete request.environmentId;
    const approval = parseApproval({id:fixture.id,...fixture.params,request})!;
    expect(approval).not.toBeNull(); // unsupported identity remains inspectable
    expect(approval.request).toEqual(request);
    const svelteEligible = canApprove(approval,evidence,'/project');
    expect(svelteEligible,testCase.name).toBe(testCase.supported);
    const receipt = receipts.find((r: {name:string}) => r.name===testCase.name);
    expect(receipt.descriptorPreserved).toBe(true);
    for (const layer of ['native','host','broker']) {
      expect(receipt[layer].accepted,`${testCase.name}/${layer}`).toBe(svelteEligible);
      expect(receipt[layer].wire).toEqual({id:fixture.id,result:{decision:svelteEligible ? 'accept' : 'decline'}});
    }
    expect(receipt.native.attemptedAccept).toBe(svelteEligible);
    expect(receipt.host.attemptedAccept).toBe(svelteEligible);
    expect(receipt.decline).toEqual({id:fixture.id,result:{decision:'decline'}});
  }
});
