/**
 * Project Store
 *
 * Reactive state management for projects
 */

import { writable, derived, get } from 'svelte/store';
import type {
  Project,
  ProjectId,
  CreateProjectRequest,
  UpdateProjectRequest,
} from '$lib/types';
import { projectService } from '$lib/services';

/** Store state interface */
interface ProjectState {
  projects: Project[];
  selectedProjectId: ProjectId | null;
  loading: boolean;
  error: string | null;
}

/** Initial state */
const initialState: ProjectState = {
  projects: [],
  selectedProjectId: null,
  loading: false,
  error: null,
};

/** Create the project store */
function createProjectStore() {
  const { subscribe, set, update } = writable<ProjectState>(initialState);

  return {
    subscribe,

    /** Load all projects */
    async load(): Promise<void> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const projects = await projectService.getAll();
        update((s) => ({ ...s, projects, loading: false }));
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to load projects',
        }));
      }
    },

    /** Load root projects only */
    async loadRoot(): Promise<void> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const projects = await projectService.getRoot();
        update((s) => ({ ...s, projects, loading: false }));
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to load projects',
        }));
      }
    },

    /** Create a new project */
    async create(request: CreateProjectRequest): Promise<Project | null> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const project = await projectService.create(request);
        update((s) => ({
          ...s,
          projects: [...s.projects, project],
          loading: false,
        }));
        return project;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to create project',
        }));
        return null;
      }
    },

    /** Update a project */
    async update(id: ProjectId, request: UpdateProjectRequest): Promise<Project | null> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const project = await projectService.update(id, request);
        update((s) => ({
          ...s,
          projects: s.projects.map((p) => (p.id === id ? project : p)),
          loading: false,
        }));
        return project;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to update project',
        }));
        return null;
      }
    },

    /** Delete a project */
    async delete(id: ProjectId): Promise<boolean> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const deleted = await projectService.delete(id);
        if (deleted) {
          update((s) => ({
            ...s,
            projects: s.projects.filter((p) => p.id !== id),
            selectedProjectId: s.selectedProjectId === id ? null : s.selectedProjectId,
            loading: false,
          }));
          return true;
        }
        update((s) => ({ ...s, loading: false }));
        return false;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to delete project',
        }));
        return false;
      }
    },

    /** Get children of a project */
    getChildren(parentId: ProjectId): Project[] {
      return get({ subscribe }).projects.filter((p) => p.parent_id === parentId);
    },

    /** Get project hierarchy (tree) */
    getProjectTree(): ProjectNode[] {
      const projects = get({ subscribe }).projects;
      const map = new Map<ProjectId, ProjectNode>();

      // Create nodes for all projects
      projects.forEach((project) => {
        map.set(project.id, { project, children: [] });
      });

      // Build tree structure
      const root: ProjectNode[] = [];
      projects.forEach((project) => {
        const node = map.get(project.id)!;
        if (project.parent_id) {
          const parent = map.get(project.parent_id);
          if (parent) {
            parent.children.push(node);
          }
        } else {
          root.push(node);
        }
      });

      return root;
    },

    /** Select a project */
    select(id: ProjectId | null): void {
      update((s) => ({ ...s, selectedProjectId: id }));
    },

    /** Get selected project ID */
    getSelectedProjectId(): ProjectId | null {
      return get({ subscribe }).selectedProjectId;
    },

    /** Clear error */
    clearError(): void {
      update((s) => ({ ...s, error: null }));
    },

    /** Reset store */
    reset(): void {
      set(initialState);
    },
  };
}

/** Project node for tree structure */
export interface ProjectNode {
  project: Project;
  children: ProjectNode[];
}

/** Project store singleton */
export const projectStore = createProjectStore();

/** Derived: root projects (no parent) */
export const rootProjects = derived(projectStore, ($store) =>
  $store.projects.filter((p) => !p.parent_id)
);

/** Derived: selected project */
export const selectedProject = derived(projectStore, ($store) =>
  $store.projects.find((p) => p.id === $store.selectedProjectId) ?? null
);

/** Derived: selected project ID */
export const selectedProjectId = derived(projectStore, ($store) => $store.selectedProjectId);

/** Derived: projects grouped by parent */
export const projectsByParent = derived(projectStore, ($store) => {
  const groups = new Map<ProjectId | null, Project[]>();
  $store.projects.forEach((project) => {
    const key = project.parent_id ?? null;
    if (!groups.has(key)) {
      groups.set(key, []);
    }
    groups.get(key)!.push(project);
  });
  return groups;
});
