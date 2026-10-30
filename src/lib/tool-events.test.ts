import { describe, it, expect } from 'vitest';
import {
  approvalToolIdentity, parseApproval, canApprove, canApproveInActiveTurn, matchesApprovalEvidence, sameApproval,
  toolIdentity, trackTool, MAX_FILE_CHANGES, MAX_TOOL_ITEMS, MAX_TOOL_DIFF_CHARS,
  MAX_TOOL_OUTPUT_CHARS, type ApprovalRequest
} from './tool-events';

const identity = { threadId: 't', turnId: 'z', itemId: 'i', startedAtMs: 1 };
const approval: ApprovalRequest = { id: 'a', kind: 'command', ...identity,
  request: { ...identity, kind:'command', environmentId:null,
    commandActions:[{ type:'unknown', command:'cargo test' }],
    command: 'cargo test', cwd: '/project', availableDecisions: ['accept', 'decline'] } };
const fileApproval: ApprovalRequest = { id: 'a', kind: 'fileChange', ...identity, request: { ...identity } };
const commandApproval = (command: string): ApprovalRequest =>
  ({ ...approval, request: { ...approval.request, command } });
const withIdentity = (request: ApprovalRequest, delta: Partial<typeof identity>): ApprovalRequest =>
  ({ ...request, ...delta, request: { ...request.request, ...delta } });
const change = (path: string, diff: unknown): Record<string, unknown> => ({ path, diff });
const itemEvent = (threadId: string, turnId: string, id: string, type: string, data: Record<string, unknown> = {}) =>
  ({ threadId, turnId, item: { id, type, ...(type === 'commandExecution' ? { cwd: '/project' } : {}), ...data } });
const fileEvent = (changes: unknown, data: Record<string, unknown> = {}, threadId = 't', turnId = 'z', id = 'i') =>
  itemEvent(threadId, turnId, id, 'fileChange', { changes, ...data });

describe('NC-07 bounded tool evidence and explicit approvals', () => {
  it('rejects malformed or forged approval identity', () => {
    expect(parseApproval(approval)).toEqual(approval);
    expect(parseApproval({ ...approval, itemId: '' })).toBeNull();
    expect(parseApproval({ ...approval, id: -1 })).toBeNull();
    expect(parseApproval({ ...approval, startedAtMs: Number.NaN })).toBeNull();
    expect(parseApproval({ ...approval, request: undefined })).toBeNull();
    expect(parseApproval({ ...approval, request: { ...approval.request, command: 'changed' } })).not.toBeNull();
    expect(sameApproval(approval, { ...approval, startedAtMs: 2 })).toBe(false);
    expect(sameApproval(approval, { ...approval, request: { ...approval.request, command: 'changed' } })).toBe(false);
  });

  it('allows a complete two-file preview only when ALL diffs are represented', () => {
    const changes = [change('src/a.ts', '-oldA\n+newA'), change('src/b.ts', '-oldB\n+newB')];
    const evidence = trackTool([], 'itemStarted', fileEvent(changes, { totalChanges: 2 }));
    expect(evidence[0].paths).toEqual(['src/a.ts', 'src/b.ts']);
    expect(evidence[0].diff).toContain('-oldA\n+newA');
    expect(evidence[0].diff).toContain('-oldB\n+newB');
    expect(evidence[0].previewComplete).toBe(true);
    expect(evidence[0].truncated).toBe(false);
    expect(canApprove(fileApproval, evidence, '/project')).toBe(true);
  });

  it.each([
    ['missing second diff', [change('src/a.ts', '+a'), { path: 'src/b.ts' }]],
    ['null second diff', [change('src/a.ts', '+a'), change('src/b.ts', null)]],
    ['empty second diff', [change('src/a.ts', '+a'), change('src/b.ts', '')]],
    ['whitespace second diff', [change('src/a.ts', '+a'), change('src/b.ts', '   ')]],
    ['missing path', [change('src/a.ts', '+a'), { diff: '+b' }]],
    ['malformed second entry', [change('src/a.ts', '+a'), 'not an object']],
    ['duplicate path', [change('src/a.ts', '+a'), change('src/a.ts', '+b')]],
    ['entry flagged partial', [change('src/a.ts', '+a'), { path: 'src/b.ts', diff: '+b', partial: true }]]
  ])('fails closed on multi-file %s', (_name, changes) => {
    const items = trackTool([], 'itemStarted', fileEvent(changes));
    expect(items[0].previewComplete).toBe(false);
    expect(items[0].truncated).toBe(true);
    expect(canApprove(fileApproval, items, '/project')).toBe(false);
  });

  it('never interprets one nonempty diff as proof of all declared files', () => {
    const first = change('src/a.ts', '+a');
    for (const data of [{ totalChanges: 2 }, { changeCount: 2 }, { hasMore: true }, { partial: true }, { truncated: true }]) {
      const tools = trackTool([], 'itemStarted', fileEvent([first], data));
      expect(canApprove(fileApproval, tools, '/project')).toBe(false);
    }
    for (const bad of [undefined, null, [], {}, 'patch text']) {
      expect(canApprove(fileApproval, trackTool([], 'itemStarted', fileEvent(bad)), '/project')).toBe(false);
    }
    const good = trackTool([], 'itemStarted', fileEvent([first, change('src/b.ts', '+b')]));
    const nowIncomplete = trackTool(good, 'itemCompleted', fileEvent([first, { path: 'src/b.ts' }]));
    expect(canApprove(fileApproval, good, '/project')).toBe(true);
    expect(canApprove(fileApproval, nowIncomplete, '/project')).toBe(false);
  });

  it('rejects per-item overflow and aggregate overflow across otherwise-valid files', () => {
    const first = change('src/a.ts', 'x'.repeat(Math.floor(MAX_TOOL_DIFF_CHARS / 2)));
    const second = change('src/b.ts', 'y'.repeat(Math.floor(MAX_TOOL_DIFF_CHARS / 2)));
    const twoFiles = trackTool([], 'itemStarted', fileEvent([first, second]));
    expect(twoFiles[0].diff.length).toBeLessThanOrEqual(MAX_TOOL_DIFF_CHARS);
    expect(twoFiles[0].truncated).toBe(true);
    expect(canApprove(fileApproval, twoFiles, '/project')).toBe(false);
    const tooMany = Array.from({ length: MAX_FILE_CHANGES + 1 }, (_, n) => change(`file-${n}`, '+a'));
    expect(canApprove(fileApproval, trackTool([], 'itemStarted', fileEvent(tooMany)), '/project')).toBe(false);
  });

  it('binds preview evidence to exact thread, turn and item, not reused item ID', () => {
    const command = itemEvent('t', 'turn-one', 'i', 'commandExecution', { command: 'cargo test' });
    const first = trackTool([], 'itemStarted', command);
    expect(canApprove(withIdentity(approval, { turnId: 'turn-one' }), first, '/project')).toBe(true);
    expect(canApprove(withIdentity(approval, { turnId: 'turn-two' }), first, '/project')).toBe(false);
    expect(canApprove(withIdentity(approval, { threadId: 'other', turnId: 'turn-one' }), first, '/project')).toBe(false);
    expect(canApprove(withIdentity(approval, { itemId: 'other', turnId: 'turn-one' }), first, '/project')).toBe(false);
    expect(matchesApprovalEvidence(withIdentity(approval, { turnId: 'turn-one' }), first[0])).toBe(true);
    expect(toolIdentity(first[0])).toBe(approvalToolIdentity(withIdentity(approval, { turnId: 'turn-one' })));
    const second = trackTool(first, 'itemStarted', itemEvent('t', 'turn-two', 'i', 'commandExecution', {}));
    expect(second).toHaveLength(2);
    expect(canApprove(withIdentity(approval, { turnId: 'turn-one' }), second, '/project')).toBe(true);
    expect(canApprove(withIdentity(approval, { turnId: 'turn-two' }), second, '/project')).toBe(false);
    expect(trackTool(first, 'commandOutput', { threadId: 't', turnId: 'turn-two', itemId: 'i', delta: 'fake' })).toEqual(first);
  });

  it('revokes Allow once when the selected active turn changes', () => {
    const tools = trackTool([], 'itemStarted', fileEvent([
      change('src/a.ts', '+a'), change('src/b.ts', '+b')
    ]));
    expect(canApproveInActiveTurn(fileApproval, tools, 't', 'z', '/project')).toBe(true);
    expect(canApproveInActiveTurn(fileApproval, tools, 't', 'next-turn', '/project')).toBe(false);
    expect(canApproveInActiveTurn(fileApproval, tools, 'other-thread', 'z', '/project')).toBe(false);
    expect(canApproveInActiveTurn(fileApproval, [], 't', 'z', '/project')).toBe(false);
  });

  it('never consumes unscoped or forged turn output as an approval preview', () => {
    const base = trackTool([], 'itemStarted', itemEvent('t', 'z', 'i', 'commandExecution', { command: 'cargo test' }));
    expect(trackTool(base, 'itemCompleted', { threadId: 't', item: { id: 'i', type: 'fileChange', changes: [change('x', '+b')] } })).toEqual(base);
    expect(trackTool(base, 'itemCompleted', fileEvent([change('x', '+b')], {}, 't', 'other', 'i'))[0]).toEqual(base[0]);
    expect(canApprove(fileApproval, base, '/project')).toBe(false);
    expect(canApprove(approval, base, '/project')).toBe(true);
  });

  it('does not misclassify final assistant and reasoning items as tools', () => {
    for (const type of ['agentMessage', 'reasoning', 'userMessage']) {
      expect(trackTool([], 'itemStarted', itemEvent('t', 'z', type, type, {}))).toEqual([]);
      expect(trackTool([], 'itemCompleted', itemEvent('t', 'z', type, type, { text: 'OK' }))).toEqual([]);
    }
    const command = trackTool([], 'itemStarted', itemEvent('t', 'z', 'cmd', 'commandExecution', { command: 'echo ok' }));
    expect(command).toHaveLength(1);
    expect(trackTool(command, 'itemCompleted', itemEvent('t', 'z', 'reply', 'agentMessage', { text: 'OK' }))).toEqual(command);
  });

  it('refuses blind/truncated command approvals and bounds output and item count', () => {
    expect(canApprove(approval, [], '/project')).toBe(false);
    expect(canApprove(approval, trackTool([], 'itemStarted',
      itemEvent('t', 'z', 'i', 'commandExecution', {})), '/project')).toBe(false);
    let tools = trackTool([], 'itemStarted',
      itemEvent('t', 'z', 'i', 'commandExecution', { command: '<script>alert(1)</script>' }));
    expect(canApprove(commandApproval('<script>alert(1)</script>'), tools, '/project')).toBe(true);
    tools = trackTool(tools, 'commandOutput',
      { threadId: 't', turnId: 'z', itemId: 'i', delta: 'a'.repeat(MAX_TOOL_OUTPUT_CHARS + 100) });
    expect(tools[0].output).toHaveLength(MAX_TOOL_OUTPUT_CHARS);
    expect(tools[0].truncated).toBe(true);
    expect(canApprove(commandApproval('<script>alert(1)</script>'), tools, '/project')).toBe(false);
    for (let n = 0; n < 30; n++) {
      tools = trackTool(tools, 'itemStarted',
        itemEvent('t', 'z', 'item' + n, 'commandExecution', { command: 'ls' }));
    }
    expect(tools).toHaveLength(MAX_TOOL_ITEMS);
    expect(tools.some(t => t.id === 'i')).toBe(false);
  });

  it('binds command, canonical project, exact scope and the actual reviewed item', () => {
    const tools = trackTool([], 'itemStarted', itemEvent('t','z','i','commandExecution',
      { command:'cargo test', cwd:'/project' }));
    expect(canApprove(approval,tools,'/project')).toBe(true);
    expect(canApprove(commandApproval('rm -rf /'),tools,'/project')).toBe(false);
    expect(canApprove(approval,tools,'/foreign')).toBe(false);
    expect(canApprove({ ...approval, request:{ ...approval.request, cwd:'/foreign' } }, tools,'/project')).toBe(false);
    for (const key of ['additionalPermissions','networkApprovalContext','proposedExecpolicyAmendment','proposedNetworkPolicyAmendments',
      'permissionScope','unexpectedField','permissionGrant','requestedPermissions','permissionProfile']) {
      expect(canApprove({ ...approval, request:{ ...approval.request, [key]: { enabled:true } } }, tools,'/project')).toBe(false);
    }
    expect(canApprove({ ...approval, request:{ ...approval.request, availableDecisions:null } },tools,'/project')).toBe(true);
    expect(canApprove({ ...approval, request:{ ...approval.request, availableDecisions:['acceptForSession'] } },tools,'/project')).toBe(false);
    expect(canApprove({ ...approval, request:{ ...approval.request, availableDecisions:['accept','acceptForSession'] } },tools,'/project')).toBe(true);
    expect(canApprove({ ...approval, request:{ ...approval.request, availableDecisions:['accept', 7] } },tools,'/project')).toBe(false);
    expect(canApprove({ ...approval, request:{ ...approval.request, availableDecisions:[
      'accept',
      { acceptWithExecpolicyAmendment:{ execpolicy_amendment:['cargo','test'] } },
      { applyNetworkPolicyAmendment:{
        network_policy_amendment:{ action:'allow', host:'example.com' }
      } },
      'decline','cancel'
    ] } },tools,'/project')).toBe(true);
    for (const choice of [
      { acceptWithExecpolicyAmendment:{ execpolicy_amendment:['cargo','test'], source:'server' } },
      { applyNetworkPolicyAmendment:{ network_policy_amendment:{ action:'allow', host:'example.com', source:'server' } } },
      { applyNetworkPolicyAmendment:{ network_policy_amendment:{ action:'allow', host:'example.com' }, source:'server' } },
      { acceptWithExecpolicyAmendment:{ execpolicy_amendment:['cargo','test'] }, decline:null }
    ]) {
      expect(canApprove({ ...approval, request:{ ...approval.request, availableDecisions:['accept',choice] } },tools,'/project')).toBe(false);
    }
    expect(canApprove({ ...approval, request:{ ...approval.request, command:undefined } },tools,'/project')).toBe(false);
    const changed = trackTool(tools,'itemCompleted',itemEvent('t','z','i','commandExecution',
      { command:'rm -rf /', cwd:'/project' }));
    expect(canApprove(approval,changed,'/project')).toBe(false);
    expect(canApprove(commandApproval('rm -rf /'),changed,'/project')).toBe(true);
  });

  it('rejects ambiguous argv, partial snapshots and unsafe file paths', () => {
    const array = trackTool([], 'itemStarted', itemEvent('t','z','i','commandExecution',
      { command:['echo','hello world'], cwd:'/project' }));
    expect(canApprove(commandApproval('echo hello world'),array,'/project')).toBe(false);
    const argv = trackTool([], 'itemStarted', itemEvent('t','z','i','commandExecution',
      { command:['echo','safe'], cwd:'/project' }));
    expect(canApprove(commandApproval('echo safe'),argv,'/project')).toBe(false);
    const matchingVector = { ...approval, request: { ...approval.request, command: ['echo','safe'] } };
    expect(canApprove(matchingVector,argv,'/project')).toBe(true);
    const partial = trackTool(argv,'itemCompleted',itemEvent('t','z','i','commandExecution',
      { cwd:'/project' }));
    expect(canApprove(commandApproval('echo safe'),partial,'/project')).toBe(false);
    for (const path of ['../escape','/absolute','a//b','a/./b','C:/foreign','a\\\\b']) {
      const tools = trackTool([], 'itemStarted', fileEvent([change(path,'+evil')]));
      expect(canApprove(fileApproval,tools,'/project')).toBe(false);
    }
    const files = trackTool([], 'itemStarted', fileEvent([change('src/safe.txt','+hello')]));
    expect(canApprove(fileApproval,files,'/project')).toBe(true);
    expect(canApprove({ ...fileApproval, request:{ ...fileApproval.request, grantRoot:'/foreign' } },files,'/project')).toBe(false);
  });


  it('accepts bounded execpolicy proposal metadata without changing the immutable request', () => {
    const tools = trackTool([], 'itemStarted', itemEvent('t','z','i','commandExecution',
      { command:'cargo test', cwd:'/project' }));
    const original = { ...approval, request:{ ...approval.request, proposedExecpolicyAmendment:['cargo','test'] } };
    const parsed = parseApproval(original);
    expect(parsed).not.toBeNull();
    expect(parsed?.request).toEqual(original.request);
    expect(canApprove(parsed!, tools, '/project')).toBe(true);
    expect(canApprove({ ...approval, request:{ ...approval.request, proposedExecpolicyAmendment:null } }, tools, '/project')).toBe(true);
    expect(canApprove({ ...approval, request:{ ...approval.request, reason:null } }, tools, '/project')).toBe(true);
    expect(canApprove(approval, tools, '/project')).toBe(true);
    expect(canApprove({ ...approval, request:{ ...approval.request, commandActions:[
      { type:'read', command:'cat file', name:'file', path:'file' },
      { type:'listFiles', command:'ls', path:null },
      { type:'search', command:'grep', query:'needle', path:null }
    ] } }, tools, '/project')).toBe(true);
    expect(canApprove({ ...approval, request:{ ...approval.request, kind:'writeStdin' } }, tools, '/project')).toBe(false);
    expect(canApprove({ ...approval, request:{ ...approval.request, environmentId:'remote' } }, tools, '/project')).toBe(false);
    expect(canApprove({ ...approval, request:{ ...approval.request, approvalId:'callback-1' } }, tools, '/project')).toBe(false);
    expect(canApprove({ ...approval, request:{ ...approval.request, commandActions:[{ type:'read', command:'cat', name:'file' }] } }, tools, '/project')).toBe(false);
    expect(sameApproval(parsed!, { ...parsed!, request:{ ...parsed!.request, proposedExecpolicyAmendment:['cargo','build'] } })).toBe(false);
  });

  it('rejects malformed or oversized execpolicy proposals and oversized descriptors', () => {
    const tools = trackTool([], 'itemStarted', itemEvent('t','z','i','commandExecution',
      { command:'cargo test', cwd:'/project' }));
    const invalid: unknown[] = [
      'cargo',
      { command:'cargo' },
      [1],
      ['x'.repeat(513)],
      Array.from({ length:33 }, () => 'x'),
      ['x'.repeat(4097)]
    ];
    for (const proposal of invalid) {
      expect(canApprove({ ...approval, request:{ ...approval.request, proposedExecpolicyAmendment:proposal } }, tools, '/project')).toBe(false);
    }
    const oversized = parseApproval({
      ...approval,
      request:{
        ...approval.request,
        command:'x'.repeat(4096),
        reason:'r'.repeat(4096)
      }
    });
    expect(oversized).toBeNull();
  });

});
