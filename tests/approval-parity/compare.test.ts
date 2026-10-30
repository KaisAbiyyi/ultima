import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';
import cases from '../fixtures/nc10g-environments.json';
import command from '../fixtures/nc10g-local-approval.json';
import fileChange from '../fixtures/nc10g-filechange-approval.json';
import { canApprove, parseApproval, trackTool } from '../../src/lib/tool-events';

for (const kind of ['command','fileChange'] as const) {
  it(`compares actual ${kind} native→broker, host→broker and both broker APIs against Svelte`, () => {
    const receiptPath=process.env.NC10G_PARITY_RECEIPT;
    if (!receiptPath) throw Error('NC10G_PARITY_RECEIPT must name the actual Rust parity receipt');
    const receipts=JSON.parse(readFileSync(receiptPath,'utf8'));
    expect(receipts).toHaveLength(cases.length*2);
    expect(new Set(receipts.map((r: {kind:string;name:string})=>`${r.kind}/${r.name}`)).size).toBe(cases.length*2);
    const fixture=kind==='command' ? command : fileChange;
    const item=kind==='command' ? {id:'i',type:'commandExecution',command:'echo safe',cwd:'/project'}
      : {id:'i',type:'fileChange',changes:[{path:'safe.txt',diff:'+safe change'}]};
    const evidence=trackTool([],'itemStarted',{threadId:'t',turnId:'u',item});
    for (const testCase of cases) {
      const request: Record<string,unknown>=structuredClone(fixture.params);
      if (testCase.present) request.environmentId=testCase.value;
      else delete request.environmentId;
      const approval=parseApproval({id:fixture.id,...fixture.params,kind,request})!;
      expect(approval).not.toBeNull();
      expect(approval.request).toEqual(request);
      const svelteEligible=canApprove(approval,evidence,'/project');
      expect(svelteEligible,testCase.name).toBe(kind==='command' ? testCase.supported : testCase.fileChangeSupported);
      const receipt=receipts.find((r: {kind:string;name:string})=>r.kind===kind&&r.name===testCase.name);
      expect(receipt.descriptorPreserved).toBe(true);
      for (const layer of ['native','host','broker','brokerDirect']) {
        expect(receipt[layer].accepted,`${kind}/${testCase.name}/${layer}`).toBe(svelteEligible);
        expect(receipt[layer].wire).toEqual({id:fixture.id,result:{decision:svelteEligible ? 'accept' : 'decline'}});
      }
      expect(receipt.native.attemptedAccept).toBe(svelteEligible);
      expect(receipt.host.attemptedAccept).toBe(svelteEligible);
      expect(receipt.decline).toEqual({id:fixture.id,result:{decision:'decline'}});
    }
  });
}
