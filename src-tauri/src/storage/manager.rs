//! SQLite Storage Manager
//!
//! Simple SQLite-based storage for agent data.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::models::{
    Agent, AgentId, AgentProvider, Chat, ChatId, ChatWithMessages, CreateAgentRequest,
    CreateChatRequest, CreateMessageRequest, CreateProjectRequest, Project, ProjectId,
    StoredChatMessage, UpdateAgentRequest, UpdateChatRequest, UpdateProjectRequest,
};
use chrono::{DateTime, Utc};

/// Storage handle type for Tauri state
pub type StorageHandle = Arc<Storage>;

/// SQLite storage manager
pub struct Storage {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Storage {
    /// Open or create storage at path
    pub fn open(path: PathBuf) -> AppResult<Self> {
        let conn = Connection::open(&path)
            .map_err(|e| AppError::Database(format!("Failed to open database: {}", e)))?;

        let storage = Self {
            conn: Mutex::new(conn),
            path,
        };

        // Initialize schema
        storage.init_schema()?;

        log::info!("SQLite storage opened at: {:?}", storage.path);

        Ok(storage)
    }

    /// Initialize database schema
    fn init_schema(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();

        // Agents table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                system_prompt TEXT NOT NULL,
                provider TEXT NOT NULL,
                model_path TEXT,
                mmproj_path TEXT,
                model_id TEXT,
                api_key TEXT,
                api_endpoint TEXT,
                context_window INTEGER NOT NULL DEFAULT 4096,
                is_aggregator INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| AppError::Database(format!("Failed to create agents table: {}", e)))?;

        // Migration: Add mmproj_path column to existing databases
        conn.execute("ALTER TABLE agents ADD COLUMN mmproj_path TEXT", [])
            .ok(); // Ignore error if column already exists

        // Chats table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS chats (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                project_id TEXT,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                is_archived INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
            )",
            [],
        )
        .map_err(|e| AppError::Database(format!("Failed to create chats table: {}", e)))?;

        // Chat messages table (normalized design - separate table for messages)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS chat_messages (
                id TEXT PRIMARY KEY,
                chat_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                images TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                usage_prompt_tokens INTEGER,
                usage_completion_tokens INTEGER,
                usage_total_tokens INTEGER,
                FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE
            )",
            [],
        )
        .map_err(|e| AppError::Database(format!("Failed to create chat_messages table: {}", e)))?;

        // Projects table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                icon TEXT,
                color TEXT,
                parent_id TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (parent_id) REFERENCES projects(id) ON DELETE SET NULL
            )",
            [],
        )
        .map_err(|e| AppError::Database(format!("Failed to create projects table: {}", e)))?;

        // Indexes for performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chats_agent ON chats(agent_id)",
            [],
        )
        .ok();

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chats_project ON chats(project_id)",
            [],
        )
        .ok();

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chats_updated ON chats(updated_at DESC)",
            [],
        )
        .ok();

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chat_messages_chat_id ON chat_messages(chat_id)",
            [],
        )
        .ok();

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_projects_parent ON projects(parent_id)",
            [],
        )
        .ok();

        Ok(())
    }

    /// Get storage path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    // === Agent CRUD Operations ===

    /// Create a new agent
    pub fn create_agent(&self, request: CreateAgentRequest) -> AppResult<Agent> {
        let agent = request.into_agent();
        agent.validate()?;

        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO agents (id, name, system_prompt, provider, model_path, mmproj_path, model_id, api_key, api_endpoint, context_window, is_aggregator, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                agent.id,
                agent.name,
                agent.system_prompt,
                agent.provider.to_string(),
                agent.model_path,
                agent.mmproj_path,
                agent.model_id,
                agent.api_key,
                agent.api_endpoint,
                agent.context_window,
                agent.is_aggregator as i32,
                agent.created_at.to_rfc3339(),
                agent.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| AppError::Database(format!("Failed to insert agent: {}", e)))?;

        log::info!("Created agent: {} ({})", agent.name, agent.id);

        Ok(agent)
    }

    /// Get agent by ID
    pub fn get_agent(&self, id: &AgentId) -> AppResult<Option<Agent>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, system_prompt, provider, model_path, mmproj_path, model_id, api_key, api_endpoint, context_window, is_aggregator, created_at, updated_at
             FROM agents WHERE id = ?1"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let result = stmt.query_row(params![id], |row| Ok(self.row_to_agent(row)));

        match result {
            Ok(agent) => Ok(Some(agent?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(format!("Query failed: {}", e))),
        }
    }

    /// Get agent by ID, error if not found
    pub fn get_agent_or_error(&self, id: &AgentId) -> AppResult<Agent> {
        self.get_agent(id)?.ok_or_else(|| AppError::NotFound {
            entity: "Agent".to_string(),
            id: id.clone(),
        })
    }

    /// Get all agents
    pub fn get_all_agents(&self) -> AppResult<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, system_prompt, provider, model_path, mmproj_path, model_id, api_key, api_endpoint, context_window, is_aggregator, created_at, updated_at
             FROM agents ORDER BY created_at DESC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map([], |row| Ok(self.row_to_agent(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut agents = Vec::new();
        for row in rows {
            let agent = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            agents.push(agent);
        }

        Ok(agents)
    }

    /// Update an agent
    pub fn update_agent(&self, id: &AgentId, request: UpdateAgentRequest) -> AppResult<Agent> {
        let mut agent = self.get_agent_or_error(id)?;
        request.apply_to(&mut agent);
        agent.validate()?;

        let conn = self.conn.lock().unwrap();

        conn.execute(
            "UPDATE agents SET name = ?2, system_prompt = ?3, provider = ?4, model_path = ?5, mmproj_path = ?6, model_id = ?7, api_key = ?8, api_endpoint = ?9, context_window = ?10, is_aggregator = ?11, updated_at = ?12
             WHERE id = ?1",
            params![
                agent.id,
                agent.name,
                agent.system_prompt,
                agent.provider.to_string(),
                agent.model_path,
                agent.mmproj_path,
                agent.model_id,
                agent.api_key,
                agent.api_endpoint,
                agent.context_window,
                agent.is_aggregator as i32,
                agent.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| AppError::Database(format!("Update failed: {}", e)))?;

        log::info!("Updated agent: {} ({})", agent.name, agent.id);

        Ok(agent)
    }

    /// Delete an agent
    pub fn delete_agent(&self, id: &AgentId) -> AppResult<bool> {
        // Check exists first
        if self.get_agent(id)?.is_none() {
            return Err(AppError::NotFound {
                entity: "Agent".to_string(),
                id: id.clone(),
            });
        }

        let conn = self.conn.lock().unwrap();

        let rows = conn
            .execute("DELETE FROM agents WHERE id = ?1", params![id])
            .map_err(|e| AppError::Database(format!("Delete failed: {}", e)))?;

        log::info!("Deleted agent: {}", id);

        Ok(rows > 0)
    }

    /// Count all agents
    pub fn count_agents(&self) -> AppResult<usize> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agents", [], |row| row.get(0))
            .map_err(|e| AppError::Database(format!("Count failed: {}", e)))?;

        Ok(count as usize)
    }

    /// Check if agent exists
    pub fn agent_exists(&self, id: &AgentId) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap();

        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agents WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(|e| AppError::Database(format!("Exists check failed: {}", e)))?;

        Ok(exists > 0)
    }

    /// Search agents by name
    pub fn search_agents(&self, query: &str) -> AppResult<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);

        let mut stmt = conn.prepare(
            "SELECT id, name, system_prompt, provider, model_path, mmproj_path, model_id, api_key, api_endpoint, context_window, is_aggregator, created_at, updated_at
             FROM agents WHERE name LIKE ?1 ORDER BY created_at DESC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map(params![pattern], |row| Ok(self.row_to_agent(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut agents = Vec::new();
        for row in rows {
            let agent = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            agents.push(agent);
        }

        Ok(agents)
    }

    /// Get agents by provider
    pub fn get_agents_by_provider(&self, provider: &str) -> AppResult<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, system_prompt, provider, model_path, mmproj_path, model_id, api_key, api_endpoint, context_window, is_aggregator, created_at, updated_at
             FROM agents WHERE provider = ?1 ORDER BY created_at DESC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map(params![provider], |row| Ok(self.row_to_agent(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut agents = Vec::new();
        for row in rows {
            let agent = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            agents.push(agent);
        }

        Ok(agents)
    }

    /// Get aggregator agents
    pub fn get_aggregator_agents(&self) -> AppResult<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, system_prompt, provider, model_path, mmproj_path, model_id, api_key, api_endpoint, context_window, is_aggregator, created_at, updated_at
             FROM agents WHERE is_aggregator = 1 ORDER BY created_at DESC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map([], |row| Ok(self.row_to_agent(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut agents = Vec::new();
        for row in rows {
            let agent = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            agents.push(agent);
        }

        Ok(agents)
    }

    // === Chat CRUD Operations ===

    /// Create a new chat
    pub fn create_chat(&self, request: CreateChatRequest) -> AppResult<Chat> {
        let chat = request.into_chat();
        chat.validate()?;

        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO chats (id, title, agent_id, project_id, is_pinned, is_archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                chat.id,
                chat.title,
                chat.agent_id,
                chat.project_id,
                chat.is_pinned as i32,
                chat.archived as i32,
                chat.created_at.to_rfc3339(),
                chat.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| AppError::Database(format!("Failed to insert chat: {}", e)))?;

        log::info!("Created chat: {} ({})", chat.title, chat.id);

        Ok(chat)
    }

    /// Get chat by ID
    pub fn get_chat(&self, id: &ChatId) -> AppResult<Option<Chat>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, title, agent_id, project_id, is_pinned, is_archived, created_at, updated_at
             FROM chats WHERE id = ?1"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let result = stmt.query_row(params![id], |row| Ok(self.row_to_chat(row)));

        match result {
            Ok(chat) => Ok(Some(chat?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(format!("Query failed: {}", e))),
        }
    }

    /// Get chat by ID, error if not found
    pub fn get_chat_or_error(&self, id: &ChatId) -> AppResult<Chat> {
        self.get_chat(id)?.ok_or_else(|| AppError::NotFound {
            entity: "Chat".to_string(),
            id: id.clone(),
        })
    }

    /// Get all non-archived chats, sorted by updated_at DESC
    pub fn get_all_chats(&self) -> AppResult<Vec<Chat>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, title, agent_id, project_id, is_pinned, is_archived, created_at, updated_at
             FROM chats
             WHERE is_archived = 0
             ORDER BY is_pinned DESC, updated_at DESC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map([], |row| Ok(self.row_to_chat(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut chats = Vec::new();
        for row in rows {
            let chat = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            chats.push(chat);
        }

        Ok(chats)
    }

    /// Get chats by agent ID
    pub fn get_chats_by_agent(&self, agent_id: &str) -> AppResult<Vec<Chat>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, title, agent_id, project_id, is_pinned, is_archived, created_at, updated_at
             FROM chats
             WHERE agent_id = ?1 AND is_archived = 0
             ORDER BY is_pinned DESC, updated_at DESC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map(params![agent_id], |row| Ok(self.row_to_chat(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut chats = Vec::new();
        for row in rows {
            let chat = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            chats.push(chat);
        }

        Ok(chats)
    }

    /// Get chats by project ID
    pub fn get_chats_by_project(&self, project_id: &str) -> AppResult<Vec<Chat>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, title, agent_id, project_id, is_pinned, is_archived, created_at, updated_at
             FROM chats
             WHERE project_id = ?1 AND is_archived = 0
             ORDER BY is_pinned DESC, updated_at DESC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map(params![project_id], |row| Ok(self.row_to_chat(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut chats = Vec::new();
        for row in rows {
            let chat = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            chats.push(chat);
        }

        Ok(chats)
    }

    /// Update chat metadata
    pub fn update_chat(&self, id: &ChatId, request: UpdateChatRequest) -> AppResult<Chat> {
        let mut chat = self.get_chat_or_error(id)?;
        request.apply_to(&mut chat);
        chat.validate()?;

        let conn = self.conn.lock().unwrap();

        conn.execute(
            "UPDATE chats SET title = ?2, project_id = ?3, is_pinned = ?4, is_archived = ?5, updated_at = ?6
             WHERE id = ?1",
            params![
                chat.id,
                chat.title,
                chat.project_id,
                chat.is_pinned as i32,
                chat.archived as i32,
                chat.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| AppError::Database(format!("Update failed: {}", e)))?;

        log::info!("Updated chat: {} ({})", chat.title, chat.id);

        Ok(chat)
    }

    /// Delete a chat
    pub fn delete_chat(&self, id: &ChatId) -> AppResult<bool> {
        if self.get_chat(id)?.is_none() {
            return Err(AppError::NotFound {
                entity: "Chat".to_string(),
                id: id.clone(),
            });
        }

        let conn = self.conn.lock().unwrap();

        let rows = conn
            .execute("DELETE FROM chats WHERE id = ?1", params![id])
            .map_err(|e| AppError::Database(format!("Delete failed: {}", e)))?;

        log::info!("Deleted chat: {}", id);

        Ok(rows > 0)
    }

    /// Add a message to a chat
    pub fn add_message_to_chat(
        &self,
        chat_id: &ChatId,
        request: CreateMessageRequest,
    ) -> AppResult<StoredChatMessage> {
        // Verify chat exists
        self.get_chat_or_error(chat_id)?;

        let message = request.into_message();
        message.validate()?;

        let conn = self.conn.lock().unwrap();

        // Serialize images to JSON
        let images_json = serde_json::to_string(&message.images)
            .map_err(|e| AppError::Database(format!("Failed to serialize images: {}", e)))?;

        conn.execute(
            "INSERT INTO chat_messages (id, chat_id, role, content, images, created_at, usage_prompt_tokens, usage_completion_tokens, usage_total_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                message.id,
                message.chat_id,
                message.role.to_string(),
                message.content,
                images_json,
                message.created_at.to_rfc3339(),
                message.usage.as_ref().map(|u| u.prompt_tokens as i32),
                message.usage.as_ref().map(|u| u.completion_tokens as i32),
                message.usage.as_ref().map(|u| u.total_tokens as i32),
            ],
        ).map_err(|e| AppError::Database(format!("Failed to insert message: {}", e)))?;

        // Update chat's updated_at timestamp
        conn.execute(
            "UPDATE chats SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), message.chat_id],
        )
        .ok();

        log::info!("Added message to chat: {}", message.chat_id);

        Ok(message)
    }

    /// Get all messages for a chat
    pub fn get_chat_messages(&self, chat_id: &ChatId) -> AppResult<Vec<StoredChatMessage>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, chat_id, role, content, images, created_at, usage_prompt_tokens, usage_completion_tokens, usage_total_tokens
             FROM chat_messages
             WHERE chat_id = ?1
             ORDER BY created_at ASC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map(params![chat_id], |row| Ok(self.row_to_chat_message(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut messages = Vec::new();
        for row in rows {
            let message = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            messages.push(message);
        }

        Ok(messages)
    }

    /// Get chat with all its messages
    pub fn get_chat_with_messages(&self, id: &ChatId) -> AppResult<ChatWithMessages> {
        let chat = self.get_chat_or_error(id)?;
        let messages = self.get_chat_messages(id)?;

        Ok(ChatWithMessages { chat, messages })
    }

    // === Project CRUD Operations ===

    /// Create a new project
    pub fn create_project(&self, request: CreateProjectRequest) -> AppResult<Project> {
        let project = request.into_project();
        project.validate()?;

        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO projects (id, name, description, icon, color, parent_id, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                project.id,
                project.name,
                project.description,
                project.icon,
                project.color,
                project.parent_id,
                project.sort_order,
                project.created_at.to_rfc3339(),
                project.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| AppError::Database(format!("Failed to insert project: {}", e)))?;

        log::info!("Created project: {} ({})", project.name, project.id);

        Ok(project)
    }

    /// Get project by ID
    pub fn get_project(&self, id: &ProjectId) -> AppResult<Option<Project>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, description, icon, color, parent_id, sort_order, created_at, updated_at
             FROM projects WHERE id = ?1"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let result = stmt.query_row(params![id], |row| Ok(self.row_to_project(row)));

        match result {
            Ok(project) => Ok(Some(project?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(format!("Query failed: {}", e))),
        }
    }

    /// Get project by ID, error if not found
    pub fn get_project_or_error(&self, id: &ProjectId) -> AppResult<Project> {
        self.get_project(id)?.ok_or_else(|| AppError::NotFound {
            entity: "Project".to_string(),
            id: id.clone(),
        })
    }

    /// Get all projects
    pub fn get_all_projects(&self) -> AppResult<Vec<Project>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, description, icon, color, parent_id, sort_order, created_at, updated_at
             FROM projects ORDER BY sort_order ASC, name ASC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map([], |row| Ok(self.row_to_project(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut projects = Vec::new();
        for row in rows {
            let project = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            projects.push(project);
        }

        Ok(projects)
    }

    /// Get root-level projects (no parent)
    pub fn get_root_projects(&self) -> AppResult<Vec<Project>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, description, icon, color, parent_id, sort_order, created_at, updated_at
             FROM projects WHERE parent_id IS NULL
             ORDER BY sort_order ASC, name ASC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map([], |row| Ok(self.row_to_project(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut projects = Vec::new();
        for row in rows {
            let project = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            projects.push(project);
        }

        Ok(projects)
    }

    /// Get child projects of a parent
    pub fn get_child_projects(&self, parent_id: &ProjectId) -> AppResult<Vec<Project>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, name, description, icon, color, parent_id, sort_order, created_at, updated_at
             FROM projects WHERE parent_id = ?1
             ORDER BY sort_order ASC, name ASC"
        ).map_err(|e| AppError::Database(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map(params![parent_id], |row| Ok(self.row_to_project(row)))
            .map_err(|e| AppError::Database(format!("Query failed: {}", e)))?;

        let mut projects = Vec::new();
        for row in rows {
            let project = row.map_err(|e| AppError::Database(format!("Row error: {}", e)))??;
            projects.push(project);
        }

        Ok(projects)
    }

    /// Update project
    pub fn update_project(
        &self,
        id: &ProjectId,
        request: UpdateProjectRequest,
    ) -> AppResult<Project> {
        let mut project = self.get_project_or_error(id)?;

        // Check for circular reference if parent is changing
        if let Some(ref new_parent_id) = request.parent_id {
            if self.would_create_circular_reference(id, new_parent_id)? {
                return Err(AppError::Validation {
                    field: "parent_id".to_string(),
                    message: "Cannot set parent: would create circular reference".to_string(),
                });
            }
        }

        request.apply_to(&mut project);
        project.validate()?;

        let conn = self.conn.lock().unwrap();

        conn.execute(
            "UPDATE projects SET name = ?2, description = ?3, icon = ?4, color = ?5, parent_id = ?6, sort_order = ?7, updated_at = ?8
             WHERE id = ?1",
            params![
                project.id,
                project.name,
                project.description,
                project.icon,
                project.color,
                project.parent_id,
                project.sort_order,
                project.updated_at.to_rfc3339(),
            ],
        ).map_err(|e| AppError::Database(format!("Update failed: {}", e)))?;

        log::info!("Updated project: {} ({})", project.name, project.id);

        Ok(project)
    }

    /// Delete a project (chats will have project_id set to NULL)
    pub fn delete_project(&self, id: &ProjectId) -> AppResult<bool> {
        if self.get_project(id)?.is_none() {
            return Err(AppError::NotFound {
                entity: "Project".to_string(),
                id: id.clone(),
            });
        }

        let conn = self.conn.lock().unwrap();

        // First, update child projects to have no parent
        conn.execute(
            "UPDATE projects SET parent_id = NULL WHERE parent_id = ?1",
            params![id],
        )
        .map_err(|e| AppError::Database(format!("Failed to update children: {}", e)))?;

        // Delete the project
        let rows = conn
            .execute("DELETE FROM projects WHERE id = ?1", params![id])
            .map_err(|e| AppError::Database(format!("Delete failed: {}", e)))?;

        log::info!("Deleted project: {}", id);

        Ok(rows > 0)
    }

    /// Check if setting new_parent_id would create a circular reference
    fn would_create_circular_reference(
        &self,
        id: &ProjectId,
        new_parent_id: &ProjectId,
    ) -> AppResult<bool> {
        // Cannot be your own parent
        if id == new_parent_id {
            return Ok(true);
        }

        // Walk up the parent chain
        let mut current_parent = Some(new_parent_id.clone());

        while let Some(ref parent_id) = current_parent {
            if parent_id == id {
                return Ok(true); // Found circular reference
            }

            let parent = self.get_project(parent_id)?;
            current_parent = parent.and_then(|p| p.parent_id);
        }

        Ok(false)
    }

    /// Convert row to Agent
    fn row_to_agent(&self, row: &rusqlite::Row) -> AppResult<Agent> {
        let provider_str: String = row.get(3).map_err(|e| AppError::Database(e.to_string()))?;
        let provider = parse_provider(&provider_str)?;

        let created_at_str: String = row.get(11).map_err(|e| AppError::Database(e.to_string()))?;
        let updated_at_str: String = row.get(12).map_err(|e| AppError::Database(e.to_string()))?;

        Ok(Agent {
            id: row.get(0).map_err(|e| AppError::Database(e.to_string()))?,
            name: row.get(1).map_err(|e| AppError::Database(e.to_string()))?,
            system_prompt: row.get(2).map_err(|e| AppError::Database(e.to_string()))?,
            provider,
            model_path: row.get(4).map_err(|e| AppError::Database(e.to_string()))?,
            mmproj_path: row.get(5).map_err(|e| AppError::Database(e.to_string()))?,
            model_id: row.get(6).map_err(|e| AppError::Database(e.to_string()))?,
            api_key: row.get(7).map_err(|e| AppError::Database(e.to_string()))?,
            api_endpoint: row.get(8).map_err(|e| AppError::Database(e.to_string()))?,
            context_window: row.get(9).map_err(|e| AppError::Database(e.to_string()))?,
            is_aggregator: row
                .get::<_, i32>(10)
                .map_err(|e| AppError::Database(e.to_string()))?
                != 0,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }

    /// Convert row to Chat
    fn row_to_chat(&self, row: &rusqlite::Row) -> AppResult<Chat> {
        let created_at_str: String = row.get(6).map_err(|e| AppError::Database(e.to_string()))?;
        let updated_at_str: String = row.get(7).map_err(|e| AppError::Database(e.to_string()))?;

        Ok(Chat {
            id: row.get(0).map_err(|e| AppError::Database(e.to_string()))?,
            title: row.get(1).map_err(|e| AppError::Database(e.to_string()))?,
            agent_id: row.get(2).map_err(|e| AppError::Database(e.to_string()))?,
            project_id: row.get(3).map_err(|e| AppError::Database(e.to_string()))?,
            is_pinned: row
                .get::<_, i32>(4)
                .map_err(|e| AppError::Database(e.to_string()))?
                != 0,
            archived: row
                .get::<_, i32>(5)
                .map_err(|e| AppError::Database(e.to_string()))?
                != 0,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }

    /// Convert row to ChatMessage
    fn row_to_chat_message(&self, row: &rusqlite::Row) -> AppResult<StoredChatMessage> {
        let role_str: String = row.get(2).map_err(|e| AppError::Database(e.to_string()))?;
        let role = parse_message_role(&role_str)?;

        let images_json: String = row.get(4).map_err(|e| AppError::Database(e.to_string()))?;
        let images: Vec<String> = serde_json::from_str(&images_json).unwrap_or_default();

        let created_at_str: String = row.get(5).map_err(|e| AppError::Database(e.to_string()))?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        // Parse optional usage fields
        let usage = match (
            row.get::<_, Option<i32>>(6).ok().flatten(),
            row.get::<_, Option<i32>>(7).ok().flatten(),
            row.get::<_, Option<i32>>(8).ok().flatten(),
        ) {
            (Some(prompt), Some(completion), Some(total)) => Some(crate::models::MessageUsage {
                prompt_tokens: prompt as u32,
                completion_tokens: completion as u32,
                total_tokens: total as u32,
            }),
            _ => None,
        };

        Ok(StoredChatMessage {
            id: row.get(0).map_err(|e| AppError::Database(e.to_string()))?,
            chat_id: row.get(1).map_err(|e| AppError::Database(e.to_string()))?,
            role,
            content: row.get(3).map_err(|e| AppError::Database(e.to_string()))?,
            images,
            created_at,
            usage,
        })
    }

    /// Convert row to Project
    fn row_to_project(&self, row: &rusqlite::Row) -> AppResult<Project> {
        let created_at_str: String = row.get(7).map_err(|e| AppError::Database(e.to_string()))?;
        let updated_at_str: String = row.get(8).map_err(|e| AppError::Database(e.to_string()))?;

        Ok(Project {
            id: row.get(0).map_err(|e| AppError::Database(e.to_string()))?,
            name: row.get(1).map_err(|e| AppError::Database(e.to_string()))?,
            description: row.get(2).map_err(|e| AppError::Database(e.to_string()))?,
            icon: row.get(3).map_err(|e| AppError::Database(e.to_string()))?,
            color: row.get(4).map_err(|e| AppError::Database(e.to_string()))?,
            parent_id: row.get(5).map_err(|e| AppError::Database(e.to_string()))?,
            sort_order: row.get(6).map_err(|e| AppError::Database(e.to_string()))?,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

/// Parse provider string to enum
fn parse_provider(s: &str) -> AppResult<AgentProvider> {
    match s {
        "llama-server" | "llama_server" => Ok(AgentProvider::LlamaServer),
        "ollama" => Ok(AgentProvider::Ollama),
        "openrouter" | "open_router" => Ok(AgentProvider::OpenRouter),
        "openai" | "open_ai" => Ok(AgentProvider::OpenAi),
        "anthropic" => Ok(AgentProvider::Anthropic),
        "custom" => Ok(AgentProvider::Custom),
        _ => Err(AppError::InvalidConfig(format!("Unknown provider: {}", s))),
    }
}

/// Parse message role string to enum
fn parse_message_role(s: &str) -> AppResult<crate::models::StorageMessageRole> {
    match s {
        "system" => Ok(crate::models::StorageMessageRole::System),
        "user" => Ok(crate::models::StorageMessageRole::User),
        "assistant" => Ok(crate::models::StorageMessageRole::Assistant),
        _ => Err(AppError::InvalidConfig(format!(
            "Unknown message role: {}",
            s
        ))),
    }
}

/// Initialize storage from app handle
pub fn init_storage(app_handle: &AppHandle) -> AppResult<StorageHandle> {
    let app_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Database(format!("Failed to get app data dir: {}", e)))?;

    // Ensure directory exists
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| AppError::Database(format!("Failed to create app dir: {}", e)))?;

    let storage_path = app_dir.join("ultima.db");
    let storage = Storage::open(storage_path)?;

    log::info!("SQLite storage initialized successfully");

    Ok(Arc::new(storage))
}

/// Get storage from Tauri state
pub fn get_storage(app_handle: &AppHandle) -> AppResult<StorageHandle> {
    app_handle
        .try_state::<StorageHandle>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| AppError::Database("Storage not initialized".to_string()))
}
