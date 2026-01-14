/**
 * Project type definitions
 *
 * Matches Rust Project model
 */

/** Strongly-typed ID type */
export type ProjectId = string;

/** Project entity for organizing chats */
export interface Project {
  id: ProjectId;
  name: string;
  description?: string;
  icon?: string;
  color?: string;
  parent_id?: ProjectId;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

/** Request to create a new project */
export interface CreateProjectRequest {
  name: string;
  description?: string;
  icon?: string;
  color?: string;
  parent_id?: ProjectId;
}

/** Request to update a project */
export interface UpdateProjectRequest {
  name?: string;
  description?: string;
  icon?: string;
  color?: string;
  parent_id?: ProjectId;
  sort_order?: number;
}
