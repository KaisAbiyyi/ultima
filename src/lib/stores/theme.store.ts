import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export type ThemeColor = 'rose' | 'blue' | 'violet' | 'emerald' | 'amber';

export interface ThemeOption {
    id: ThemeColor;
    name: string;
    color: string; // Preview color (primary-500)
}

export const THEME_OPTIONS: ThemeOption[] = [
    { id: 'rose', name: 'Rose', color: '#F43F5E' },
    { id: 'blue', name: 'Blue', color: '#3B82F6' },
    { id: 'violet', name: 'Violet', color: '#8B5CF6' },
    { id: 'emerald', name: 'Emerald', color: '#10B981' },
    { id: 'amber', name: 'Amber', color: '#F59E0B' },
];

const STORAGE_KEY = 'ultima_theme';
const DEFAULT_THEME: ThemeColor = 'rose';

function getInitialTheme(): ThemeColor {
    if (browser) {
        const stored = localStorage.getItem(STORAGE_KEY);
        if (stored && THEME_OPTIONS.some(t => t.id === stored)) {
            return stored as ThemeColor;
        }
    }
    return DEFAULT_THEME;
}

function createThemeStore() {
    const { subscribe, set } = writable<ThemeColor>(getInitialTheme());

    return {
        subscribe,
        setTheme: (theme: ThemeColor) => {
            if (browser) {
                document.documentElement.setAttribute('data-theme', theme);
                localStorage.setItem(STORAGE_KEY, theme);
            }
            set(theme);
        },
        init: () => {
            if (browser) {
                const theme = getInitialTheme();
                document.documentElement.setAttribute('data-theme', theme);
                set(theme);
            }
        }
    };
}

export const themeStore = createThemeStore();
