import {
  type LocalAsrTokenPayload,
  shouldAcceptLocalAsrToken,
} from './localAsr.ts';

function assertEqual<T>(actual: T, expected: T, message: string) {
  if (actual !== expected) {
    throw new Error(
      `${message}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
    );
  }
}

const token: LocalAsrTokenPayload = {
  sessionId: 'session-current',
  provider: 'local-qwen3',
  source: 'live',
  sequence: 1,
  piece: 'hello',
};

assertEqual(
  shouldAcceptLocalAsrToken(token, 'session-current'),
  true,
  'accepts token for the active capsule session',
);
assertEqual(
  shouldAcceptLocalAsrToken(token, 'session-old'),
  false,
  'rejects token from a stale session',
);
assertEqual(
  shouldAcceptLocalAsrToken(token, null),
  false,
  'rejects token when no active capsule session exists',
);

console.log('localAsrToken.test.ts passed');
