import { describe, expect, it } from 'vitest';
import { classifyTurnOutcome, turnOutcomeMessage } from './turn-outcome';

describe('official Codex terminal turn status', () => {
  it('classifies upstream usage limit without revealing server error content', () => {
    const outcome = classifyTurnOutcome({ threadId: 't', turn: { id: 'z', status: 'failed', error: { message: 'Usage limit reached for sensitive-account-identifier' } } });
    expect(outcome).toBe('usage-limit');
    expect(turnOutcomeMessage(outcome)).toContain('usage limit');
    expect(turnOutcomeMessage(outcome)).not.toContain('sensitive-account-identifier');
    expect(classifyTurnOutcome({ turn: { status: 'failed', error: { codexErrorInfo: { type: 'UsageLimitExceeded' } } } })).toBe('usage-limit');
  });
  it('distinguishes final statuses and malformed errors', () => {
    expect(classifyTurnOutcome({ turn: { status: 'completed' } })).toBe('completed');
    expect(classifyTurnOutcome({ turn: { status: 'interrupted' } })).toBe('interrupted');
    expect(classifyTurnOutcome({ turn: { status: 'failed', error: { codexErrorInfo: 'Unauthorized' } } })).toBe('unauthorized');
    expect(classifyTurnOutcome({ turn: { status: 'failed', error: { codexErrorInfo: { type: 'ResponseStreamDisconnected' } } } })).toBe('network');
    expect(classifyTurnOutcome({ turn: { status: 'failed', error: { message: 'unspecified internal error' } } })).toBe('failed');
    expect(classifyTurnOutcome({ turn: { status: 'inProgress' } })).toBe('none');
    expect(classifyTurnOutcome({})).toBe('none');
    expect(classifyTurnOutcome({ turn: { status: 'failed', error: { message: { surprise: true } } } })).toBe('failed');
  });
});
