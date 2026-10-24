export type Role = 'user' | 'assistant';

export interface UIMessage {
  id: string;
  role: Role;
  text: string;
  at: string;
}

const questions = [
  'Review the memory footprint of the desktop shell.',
  'Can we make the streaming renderer easier to maintain?',
  'Explain the lifecycle of an interrupted tool call.',
  'Show a minimal fix without adding more dependencies.',
  'What is the simplest safe fallback when the engine disconnects?'
] as const;

const replies = [
  'I would start by measuring the full process tree, then keep the UI work bounded to visible content.',
  'Separate transport events from presentation state. Batch updates and clean up every listener.',
  'A cancel request is explicit and must never silently approve or repeat a pending tool action.',
  'Keep the diff focused: one small interface, no new background polling and no duplicate runtime.',
  'Show the disconnected state, preserve the draft and let the user retry when the engine is ready.'
] as const;

/** Stable, deterministic, entirely synthetic test data. No user history or network calls. */
export function makeHistory(count = 10_000): UIMessage[] {
  if (!Number.isInteger(count) || count < 0 || count > 100_000) {
    throw new RangeError('Fixture count must be an integer between 0 and 100,000');
  }
  return Array.from({ length: count }, (_, index) => {
    const role: Role = index % 2 === 0 ? 'user' : 'assistant';
    const sequence = Math.floor(index / 2);
    return {
      id: 'fixture-' + index,
      role,
      text: role === 'user' ? questions[sequence % questions.length] : replies[sequence % replies.length],
      at: String(9 + (index % 9)).padStart(2, '0') + ':' + String(index % 60).padStart(2, '0')
    };
  });
}

export function createMessage(role: Role, text: string, index: number): UIMessage {
  return { id: 'live-' + index + '-' + role, role, text, at: 'Now' };
}
