import { writable } from 'svelte/store';

export type BreadcrumbItem = {
    label: string;
    href?: string;
    icon?: any; // Component type
};

export const breadcrumbStore = writable<BreadcrumbItem[]>([]);
