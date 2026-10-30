import { defineConfig } from 'vitest/config';
export default defineConfig({ test:{ include:['tests/approval-parity/compare.test.ts'], environment:'node' } });
