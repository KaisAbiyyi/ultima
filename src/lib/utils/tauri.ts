/**
 * Tauri Utilities
 * 
 * Safe wrappers for Tauri APIs that work in browser mode
 */

/**
 * Check if running in Tauri environment
 * Works for both Tauri v1 and v2
 */
export function isTauri(): boolean {
  if (typeof window === 'undefined') return false;

  // Tauri v2 uses __TAURI_INTERNALS__
  if ('__TAURI_INTERNALS__' in window) return true;

  // Tauri v1 uses __TAURI__
  if ('__TAURI__' in window) return true;

  return false;
}

/**
 * Safe invoke wrapper - returns null if not in Tauri
 */
export async function safeInvoke<T>(
  cmd: string, 
  args?: Record<string, unknown>
): Promise<T | null> {
  if (!isTauri()) {
    console.warn(`Tauri not available, skipping command: ${cmd}`);
    return null;
  }
  
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const result = await invoke<T>(cmd, args);
    return result;
  } catch (error) {
    console.error(`Tauri command '${cmd}' failed:`, error);
    throw error; // Re-throw so caller can handle it
  }
}

/**
 * Safe invoke wrapper with fallback
 */
export async function safeInvokeWithFallback<T>(
  cmd: string, 
  args: Record<string, unknown> | undefined,
  fallback: T
): Promise<T> {
  const result = await safeInvoke<T>(cmd, args);
  return result ?? fallback;
}
