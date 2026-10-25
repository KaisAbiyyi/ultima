/** Fixed, safe terminal failure categories; never echo upstream error text. */
export type TurnOutcome = 'none' | 'completed' | 'interrupted' | 'usage-limit' | 'unauthorized' | 'network' | 'failed';
type RecordValue = Record<string, unknown>;
const asRecord = (v: unknown): RecordValue | null =>
  v !== null && typeof v === 'object' && !Array.isArray(v) ? v as RecordValue : null;

export function classifyTurnOutcome(params: unknown): TurnOutcome {
  const turn = asRecord(asRecord(params)?.turn);
  if (!turn) return 'none';
  if (turn.status === 'completed') return 'completed';
  if (turn.status === 'interrupted') return 'interrupted';
  if (turn.status !== 'failed') return 'none';
  const error = asRecord(turn.error);
  const info = error?.codexErrorInfo;
  const infoType = typeof info === 'string' ? info :
    typeof asRecord(info)?.type === 'string' ? asRecord(info)?.type as string : '';
  const message = typeof error?.message === 'string' ? error.message.slice(0, 4096) : '';
  if (/UsageLimitExceeded|RateLimitExceeded/i.test(infoType) ||
      /usage limit|rate limit|quota exceeded|usage cap|usage limits|reached your limit/i.test(message)) return 'usage-limit';
  if (/Unauthorized|Authentication/i.test(infoType) || /unauthorized|authentication required/i.test(message)) return 'unauthorized';
  if (/HttpConnectionFailed|ResponseStreamConnectionFailed|ResponseStreamDisconnected/i.test(infoType)) return 'network';
  return 'failed';
}

export function turnOutcomeMessage(outcome: TurnOutcome): string {
  switch (outcome) {
    case 'usage-limit': return 'Codex has reached an upstream usage limit. Retry after your Codex allowance resets.';
    case 'unauthorized': return 'Codex authentication was rejected. Check your Codex account session.';
    case 'network': return 'Codex lost its upstream connection. Check connectivity before retrying.';
    case 'failed': return 'Codex reported that the turn failed. No response was produced.';
    case 'interrupted': return 'Codex turn was interrupted.';
    default: return '';
  }
}
