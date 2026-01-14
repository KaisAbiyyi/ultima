/**
 * Project Service
 *
 * Handles all project-related Tauri command invocations
 */

import { safeInvoke, safeInvokeWithFallback } from '$lib/utils/tauri';
import type {
  Project,
  ProjectId,
  CreateProjectRequest,
  UpdateProjectRequest,
} from '$lib/types';

/**
 * Project service for CRUD operations
 */
export const projectService = {
  /**
   * Create a new project
   */
  async create(request: CreateProjectRequest): Promise<Project> {
    const result = await safeInvoke<Project>('create_project', { request });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Get project by ID
   */
  async getById(id: ProjectId): Promise<Project | null> {
    return safeInvoke<Project>('get_project', { id });
  },

  /**
   * Get all projects
   */
  async getAll(): Promise<Project[]> {
    return safeInvokeWithFallback<Project[]>('get_all_projects', undefined, []);
  },

  /**
   * Get root-level projects (no parent)
   */
  async getRoot(): Promise<Project[]> {
    return safeInvokeWithFallback<Project[]>('get_root_projects', undefined, []);
  },

  /**
   * Get child projects of a parent
   */
  async getChildren(parentId: ProjectId): Promise<Project[]> {
    return safeInvokeWithFallback<Project[]>('get_child_projects', { parentId }, []);
  },

  /**
   * Update a project
   */
  async update(id: ProjectId, request: UpdateProjectRequest): Promise<Project> {
    const result = await safeInvoke<Project>('update_project', { id, request });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Delete a project
   */
  async delete(id: ProjectId): Promise<boolean> {
    return safeInvokeWithFallback<boolean>('delete_project', { id }, false);
  },

  /**
   * Validate project request
   */
  validate(request: CreateProjectRequest): string[] {
    const errors: string[] = [];

    if (!request.name?.trim()) {
      errors.push('Name is required');
    }

    if (request.name && request.name.length > 100) {
      errors.push('Name cannot exceed 100 characters');
    }

    return errors;
  },
};
