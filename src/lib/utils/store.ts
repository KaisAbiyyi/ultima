/**
 * Store Utilities
 * 
 * Helper functions to reduce code duplication in Svelte stores.
 */

import type { Writable } from 'svelte/store';

/**
 * Base state interface for stores with loading/error states
 */
export interface LoadableState {
  loading: boolean;
  error: string | null;
}

/**
 * Update function type for stores
 */
type StoreUpdate<S> = (updater: (state: S) => S) => void;

/**
 * Execute an async operation with automatic loading state management.
 * 
 * This eliminates the repetitive try/catch pattern seen throughout stores:
 * ```
 * update(s => ({ ...s, loading: true, error: null }));
 * try {
 *   const result = await operation();
 *   update(s => ({ ...s, loading: false }));
 *   return result;
 * } catch (err) {
 *   update(s => ({ ...s, loading: false, error: err.message }));
 *   return null;
 * }
 * ```
 * 
 * @example
 * ```ts
 * async load(): Promise<void> {
 *   await withLoading(update, async () => {
 *     const items = await service.getAll();
 *     update(s => ({ ...s, items }));
 *   }, 'Failed to load items');
 * }
 * ```
 */
export async function withLoading<S extends LoadableState, T>(
  update: StoreUpdate<S>,
  operation: () => Promise<T>,
  errorMessage: string
): Promise<T | null> {
  update((s) => ({ ...s, loading: true, error: null }));
  
  try {
    const result = await operation();
    update((s) => ({ ...s, loading: false }));
    return result;
  } catch (err) {
    const message = err instanceof Error ? err.message : errorMessage;
    update((s) => ({ ...s, loading: false, error: message }));
    console.error(errorMessage, err);
    return null;
  }
}

/**
 * Execute an async operation that returns a result with loading state management.
 * Unlike withLoading, this version runs the state update inside the try block.
 * 
 * @example
 * ```ts
 * async create(request: CreateRequest): Promise<Item | null> {
 *   return withLoadingResult(
 *     update,
 *     async () => {
 *       const item = await service.create(request);
 *       return { item, stateUpdater: (items) => [...items, item] };
 *     },
 *     'Failed to create item',
 *     'items'
 *   );
 * }
 * ```
 */
export async function withLoadingResult<S extends LoadableState, T>(
  update: StoreUpdate<S>,
  operation: () => Promise<{ result: T; stateUpdate: Partial<S> }>,
  errorMessage: string
): Promise<T | null> {
  update((s) => ({ ...s, loading: true, error: null }));
  
  try {
    const { result, stateUpdate } = await operation();
    update((s) => ({ ...s, ...stateUpdate, loading: false }));
    return result;
  } catch (err) {
    const message = err instanceof Error ? err.message : errorMessage;
    update((s) => ({ ...s, loading: false, error: message }));
    console.error(errorMessage, err);
    return null;
  }
}

/**
 * Parse error to a user-friendly message
 */
export function parseError(err: unknown, fallback: string): string {
  if (err instanceof Error) {
    return err.message;
  }
  if (typeof err === 'string') {
    return err;
  }
  return fallback;
}
