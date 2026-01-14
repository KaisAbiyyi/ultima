/**
 * Sidebar Store
 * 
 * Global state for sidebar collapsed state
 */

import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const STORAGE_KEY = 'ultima_sidebar_collapsed';

function createSidebarStore() {
  // Load initial state from localStorage
  const storedValue = browser ? localStorage.getItem(STORAGE_KEY) : null;
  const initialCollapsed = storedValue === 'true';
  
  const { subscribe, set, update } = writable(initialCollapsed);

  return {
    subscribe,
    
    /** Toggle collapsed state */
    toggle: () => update(collapsed => {
      const newValue = !collapsed;
      if (browser) {
        localStorage.setItem(STORAGE_KEY, String(newValue));
      }
      return newValue;
    }),
    
    /** Set collapsed state */
    setCollapsed: (collapsed: boolean) => {
      if (browser) {
        localStorage.setItem(STORAGE_KEY, String(collapsed));
      }
      set(collapsed);
    },
    
    /** Expand sidebar */
    expand: () => {
      if (browser) {
        localStorage.setItem(STORAGE_KEY, 'false');
      }
      set(false);
    },
    
    /** Collapse sidebar */
    collapse: () => {
      if (browser) {
        localStorage.setItem(STORAGE_KEY, 'true');
      }
      set(true);
    },
  };
}

export const sidebarStore = createSidebarStore();
