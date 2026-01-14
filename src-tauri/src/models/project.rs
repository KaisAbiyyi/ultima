//! Project model definition
//!
//! Represents a folder/category for organizing chats.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

// === Type Aliases ===

/// Strongly-typed Project ID
pub type ProjectId = String;

// === Main Model ===

/// A project/folder for organizing chats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Unique identifier
    pub id: ProjectId,

    /// Display name
    pub name: String,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Icon identifier (emoji or icon name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Color for visual distinction (hex code)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Parent project ID for nesting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<ProjectId>,

    /// Sort order within parent (lower = first)
    #[serde(default)]
    pub sort_order: i32,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Project {
    /// Create a new project
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: None,
            icon: None,
            color: None,
            parent_id: None,
            sort_order: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Validate project data
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation {
                field: "name".to_string(),
                message: "Project name cannot be empty".to_string(),
            });
        }

        if self.name.len() > 100 {
            return Err(AppError::Validation {
                field: "name".to_string(),
                message: "Project name cannot exceed 100 characters".to_string(),
            });
        }

        // Validate color format if provided
        if let Some(ref color) = self.color {
            if !color.starts_with('#') || color.len() != 7 {
                return Err(AppError::Validation {
                    field: "color".to_string(),
                    message: "Color must be a valid hex code (e.g., #FF5733)".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Check if this is a root-level project
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Update the name
    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self.updated_at = Utc::now();
        self
    }

    /// Set description
    pub fn with_description(mut self, description: Option<String>) -> Self {
        self.description = description;
        self.updated_at = Utc::now();
        self
    }

    /// Set icon
    pub fn with_icon(mut self, icon: Option<String>) -> Self {
        self.icon = icon;
        self.updated_at = Utc::now();
        self
    }

    /// Set color
    pub fn with_color(mut self, color: Option<String>) -> Self {
        self.color = color;
        self.updated_at = Utc::now();
        self
    }

    /// Set parent project
    pub fn with_parent_id(mut self, parent_id: Option<ProjectId>) -> Self {
        self.parent_id = parent_id;
        self.updated_at = Utc::now();
        self
    }

    /// Set sort order
    pub fn with_sort_order(mut self, sort_order: i32) -> Self {
        self.sort_order = sort_order;
        self.updated_at = Utc::now();
        self
    }
}

// === DTOs ===

/// Request to create a new project
#[derive(Debug, Clone, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub parent_id: Option<ProjectId>,
}

impl CreateProjectRequest {
    /// Convert to Project entity
    pub fn into_project(self) -> Project {
        let mut project = Project::new(self.name);
        project.description = self.description;
        project.icon = self.icon;
        project.color = self.color;
        project.parent_id = self.parent_id;
        project
    }
}

/// Request to update project
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateProjectRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub parent_id: Option<ProjectId>,
    #[serde(default)]
    pub sort_order: Option<i32>,
}

impl UpdateProjectRequest {
    /// Apply updates to a project
    pub fn apply_to(self, project: &mut Project) {
        if let Some(name) = self.name {
            project.name = name;
        }
        if self.description.is_some() {
            project.description = self.description;
        }
        if self.icon.is_some() {
            project.icon = self.icon;
        }
        if self.color.is_some() {
            project.color = self.color;
        }
        if self.parent_id.is_some() {
            project.parent_id = self.parent_id;
        }
        if let Some(order) = self.sort_order {
            project.sort_order = order;
        }
        project.updated_at = Utc::now();
    }
}

// === Tests ===

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_creation() {
        let project = Project::new("My Project");

        assert!(!project.id.is_empty());
        assert_eq!(project.name, "My Project");
        assert!(project.is_root());
        assert_eq!(project.sort_order, 0);
    }

    #[test]
    fn test_validation_empty_name() {
        let project = Project::new("");
        assert!(project.validate().is_err());
    }

    #[test]
    fn test_validation_whitespace_only_name() {
        let project = Project::new("   ");
        assert!(project.validate().is_err());
    }

    #[test]
    fn test_validation_long_name() {
        let project = Project::new("a".repeat(101));
        assert!(project.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_color() {
        let mut project = Project::new("Test");
        project.color = Some("invalid".to_string());
        assert!(project.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_color_short() {
        let mut project = Project::new("Test");
        project.color = Some("#F00".to_string());
        assert!(project.validate().is_err());
    }

    #[test]
    fn test_validation_valid_color() {
        let mut project = Project::new("Test");
        project.color = Some("#FF5733".to_string());
        assert!(project.validate().is_ok());
    }

    #[test]
    fn test_from_request() {
        let request = CreateProjectRequest {
            name: "Coding".to_string(),
            description: Some("Code projects".to_string()),
            icon: Some("💻".to_string()),
            color: Some("#3B82F6".to_string()),
            parent_id: None,
        };

        let project = request.into_project();
        assert_eq!(project.name, "Coding");
        assert_eq!(project.description, Some("Code projects".to_string()));
        assert_eq!(project.icon, Some("💻".to_string()));
        assert_eq!(project.color, Some("#3B82F6".to_string()));
    }

    #[test]
    fn test_from_request_with_parent() {
        let request = CreateProjectRequest {
            name: "Subfolder".to_string(),
            description: None,
            icon: None,
            color: None,
            parent_id: Some("parent-123".to_string()),
        };

        let project = request.into_project();
        assert_eq!(project.parent_id, Some("parent-123".to_string()));
        assert!(!project.is_root());
    }

    #[test]
    fn test_update_request() {
        let mut project = Project::new("Original");

        let update = UpdateProjectRequest {
            name: Some("Updated".to_string()),
            description: Some("New description".to_string()),
            sort_order: Some(5),
            ..Default::default()
        };

        update.apply_to(&mut project);
        assert_eq!(project.name, "Updated");
        assert_eq!(project.description, Some("New description".to_string()));
        assert_eq!(project.sort_order, 5);
    }

    #[test]
    fn test_serialization() {
        let project = Project::new("Test");
        let json = serde_json::to_string(&project).unwrap();
        let parsed: Project = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, project.id);
        assert_eq!(parsed.name, project.name);
        assert_eq!(parsed.sort_order, project.sort_order);
    }

    #[test]
    fn test_with_name() {
        let project = Project::new("Original").with_name("New Name".to_string());
        assert_eq!(project.name, "New Name");
    }

    #[test]
    fn test_with_description() {
        let project = Project::new("Test").with_description(Some("Description".to_string()));
        assert_eq!(project.description, Some("Description".to_string()));
    }

    #[test]
    fn test_with_icon() {
        let project = Project::new("Test").with_icon(Some("📁".to_string()));
        assert_eq!(project.icon, Some("📁".to_string()));
    }

    #[test]
    fn test_with_color() {
        let project = Project::new("Test").with_color(Some("#FF0000".to_string()));
        assert_eq!(project.color, Some("#FF0000".to_string()));
    }

    #[test]
    fn test_with_parent_id() {
        let project = Project::new("Test").with_parent_id(Some("parent-123".to_string()));
        assert_eq!(project.parent_id, Some("parent-123".to_string()));
        assert!(!project.is_root());
    }

    #[test]
    fn test_with_sort_order() {
        let project = Project::new("Test").with_sort_order(10);
        assert_eq!(project.sort_order, 10);
    }

    #[test]
    fn test_default_sort_order() {
        let project = Project::new("Test");
        assert_eq!(project.sort_order, 0);
    }
}
