use crate::core::errors::{AppError, AppResult};
use crate::error_analyzer::{DiagnosticFailure, FailureCategory};
use crate::planning_engine::TaskPlan;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ═══════════════════════════════════════════════════════════════
// Knowledge Entry Model
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: String,
    pub category: KnowledgeCategory,
    pub tags: Vec<String>,
    pub summary: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub access_count: u32,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KnowledgeCategory {
    Architecture,
    Dependency,
    Module,
    Symbol,
    Plan,
    FixStrategy,
    Error,
    File,
    Risk,
    Unknown,
}

impl KnowledgeCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            KnowledgeCategory::Architecture => "architecture",
            KnowledgeCategory::Dependency => "dependency",
            KnowledgeCategory::Module => "module",
            KnowledgeCategory::Symbol => "symbol",
            KnowledgeCategory::Plan => "plan",
            KnowledgeCategory::FixStrategy => "fix_strategy",
            KnowledgeCategory::Error => "error",
            KnowledgeCategory::File => "file",
            KnowledgeCategory::Risk => "risk",
            KnowledgeCategory::Unknown => "unknown",
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Knowledge Store
// ═══════════════════════════════════════════════════════════════

pub struct KnowledgeStore;

impl KnowledgeStore {
    pub fn schema() -> &'static str {
        r#"
CREATE TABLE IF NOT EXISTS knowledge_entries (
    id TEXT PRIMARY KEY,
    category TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    summary TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    access_count INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_knowledge_category ON knowledge_entries(category);
CREATE INDEX IF NOT EXISTS idx_knowledge_updated ON knowledge_entries(updated_at);
CREATE INDEX IF NOT EXISTS idx_knowledge_access ON knowledge_entries(access_count);
"#
    }

    pub fn ensure_schema(conn: &Connection) -> AppResult<()> {
        conn.execute_batch(Self::schema())
            .map_err(|e| AppError::Provider(format!("knowledge_store schema: {e}")))
    }

    pub fn insert(conn: &Connection, entry: &KnowledgeEntry) -> AppResult<()> {
        let tags_json = serde_json::to_string(&entry.tags).unwrap_or_else(|_| "[]".to_string());
        let now = epoch_secs();

        conn.execute(
            "INSERT INTO knowledge_entries (id, category, tags, summary, content, created_at, updated_at, access_count, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id,
                entry.category.as_str(),
                tags_json,
                entry.summary,
                entry.content,
                now,
                now,
                entry.access_count,
                entry.version.max(1),
            ],
        )
        .map_err(|e| AppError::Provider(format!("knowledge_store insert: {e}")))?;
        Ok(())
    }

    pub fn upsert(conn: &Connection, entry: &KnowledgeEntry) -> AppResult<()> {
        let existing = Self::get_by_id(conn, &entry.id).ok();
        let tags_json = serde_json::to_string(&entry.tags).unwrap_or_else(|_| "[]".to_string());
        let now = epoch_secs();

        if let Some(ref old) = existing {
            let new_version = old.version + 1;
            let merged_content = if old.content == entry.content {
                old.content.clone()
            } else {
                format!("{}\n--- updated v{} ---\n{}", old.content, new_version, entry.content)
            };

            let merged_tags = if entry.tags.is_empty() { old.tags.clone() } else { entry.tags.clone() };
            let merged_tags_json = serde_json::to_string(&merged_tags).unwrap_or_else(|_| "[]".to_string());

            conn.execute(
                "UPDATE knowledge_entries SET category=?1, tags=?2, summary=?3, content=?4, updated_at=?5, access_count=access_count+1, version=?6 WHERE id=?7",
                params![entry.category.as_str(), merged_tags_json, entry.summary, merged_content, now, new_version, entry.id],
            )
            .map_err(|e| AppError::Provider(format!("knowledge_store upsert: {e}")))?;
        } else {
            return Self::insert(conn, entry);
        }
        Ok(())
    }

    pub fn get_by_id(conn: &Connection, id: &str) -> AppResult<KnowledgeEntry> {
        conn.query_row(
            "SELECT id, category, tags, summary, content, created_at, updated_at, access_count, version FROM knowledge_entries WHERE id = ?1",
            params![id],
            |row| {
                let tags_str: String = row.get(2)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                Ok(KnowledgeEntry {
                    id: row.get(0)?,
                    category: parse_category(&row.get::<_, String>(1)?),
                    tags,
                    summary: row.get(3)?,
                    content: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    access_count: row.get(7)?,
                    version: row.get(8)?,
                })
            },
        )
        .map_err(|e| AppError::Provider(format!("knowledge_store get_by_id: {e}")))
    }

    pub fn query_by_category(
        conn: &Connection,
        category: &KnowledgeCategory,
        limit: usize,
    ) -> AppResult<Vec<KnowledgeEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, category, tags, summary, content, created_at, updated_at, access_count, version
             FROM knowledge_entries WHERE category = ?1 ORDER BY updated_at DESC LIMIT ?2",
        )
        .map_err(|e| AppError::Provider(format!("knowledge_store query: {e}")))?;

        let rows = stmt.query_map(params![category.as_str(), limit as i64], |row| {
            let tags_str: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(KnowledgeEntry {
                id: row.get(0)?,
                category: parse_category(&row.get::<_, String>(1)?),
                tags,
                summary: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                access_count: row.get(7)?,
                version: row.get(8)?,
            })
        })
        .map_err(|e| AppError::Provider(format!("knowledge_store query_map: {e}")))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn search(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<KnowledgeEntry>> {
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT id, category, tags, summary, content, created_at, updated_at, access_count, version
             FROM knowledge_entries WHERE summary LIKE ?1 OR content LIKE ?1 OR tags LIKE ?1
             ORDER BY access_count DESC, updated_at DESC LIMIT ?2",
        )
        .map_err(|e| AppError::Provider(format!("knowledge_store search: {e}")))?;

        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            let tags_str: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(KnowledgeEntry {
                id: row.get(0)?,
                category: parse_category(&row.get::<_, String>(1)?),
                tags,
                summary: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                access_count: row.get(7)?,
                version: row.get(8)?,
            })
        })
        .map_err(|e| AppError::Provider(format!("knowledge_store search_map: {e}")))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn find_fix_strategies(
        conn: &Connection,
        error_category: &FailureCategory,
        limit: usize,
    ) -> AppResult<Vec<KnowledgeEntry>> {
        let tag = error_category.as_str().to_string();
        let pattern = format!("%{}%", tag);
        let mut stmt = conn.prepare(
            "SELECT id, category, tags, summary, content, created_at, updated_at, access_count, version
             FROM knowledge_entries WHERE (category = 'fix_strategy' OR category = 'error')
             AND (tags LIKE ?1 OR summary LIKE ?1)
             ORDER BY access_count DESC, updated_at DESC LIMIT ?2",
        )
        .map_err(|e| AppError::Provider(format!("knowledge_store find_fix: {e}")))?;

        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            let tags_str: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(KnowledgeEntry {
                id: row.get(0)?,
                category: parse_category(&row.get::<_, String>(1)?),
                tags,
                summary: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                access_count: row.get(7)?,
                version: row.get(8)?,
            })
        })
        .map_err(|e| AppError::Provider(format!("knowledge_store find_fix_map: {e}")))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn expire_stale(conn: &Connection, max_age_secs: u64, min_access_count: u32) -> AppResult<usize> {
        let cutoff = epoch_secs() - max_age_secs as i64;
        let deleted = conn.execute(
            "DELETE FROM knowledge_entries WHERE updated_at < ?1 AND access_count < ?2 AND category != 'architecture'",
            params![cutoff, min_access_count],
        )
        .map_err(|e| AppError::Provider(format!("knowledge_store expire: {e}")))?;
        Ok(deleted)
    }

    pub fn deduplicate(conn: &Connection) -> AppResult<usize> {
        let mut merged = 0usize;
        let entries: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT id, summary FROM knowledge_entries ORDER BY updated_at DESC")
                .map_err(|e| AppError::Provider(format!("knowledge_store dedup_scan: {e}")))?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map_err(|e| AppError::Provider(format!("knowledge_store dedup_map: {e}")))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                if entries[i].1 == entries[j].1 {
                    let content_i: String = conn.query_row("SELECT content FROM knowledge_entries WHERE id = ?1", params![entries[i].0], |r| r.get(0)).unwrap_or_default();
                    let content_j: String = conn.query_row("SELECT content FROM knowledge_entries WHERE id = ?1", params![entries[j].0], |r| r.get(0)).unwrap_or_default();
                    let merged_content = format!("{}\n{}", content_i, content_j);
                    let _ = conn.execute("UPDATE knowledge_entries SET content = ?1, version = version + 1 WHERE id = ?2", params![merged_content, entries[i].0]);
                    let _ = conn.execute("DELETE FROM knowledge_entries WHERE id = ?1", params![entries[j].0]);
                    merged += 1;
                }
            }
        }
        Ok(merged)
    }

    // ── Agent Integration Hooks ──

    pub fn record_plan(conn: &Connection, plan: &TaskPlan) -> AppResult<()> {
        let entry = KnowledgeEntry {
            id: format!("plan-{}", epoch_secs()),
            category: KnowledgeCategory::Plan,
            tags: plan.affected_files.clone(),
            summary: format!("Plan: {}", plan.task_description),
            content: serde_json::to_string_pretty(plan).unwrap_or_else(|_| plan.task_description.clone()),
            created_at: epoch_secs(),
            updated_at: epoch_secs(),
            access_count: 0,
            version: 1,
        };
        Self::insert(conn, &entry)
    }

    pub fn record_failure(conn: &Connection, failure: &DiagnosticFailure) -> AppResult<()> {
        let entry = KnowledgeEntry {
            id: format!("error-{}-{}", failure.category.as_str(), epoch_secs()),
            category: KnowledgeCategory::Error,
            tags: vec![failure.category.as_str().to_string(), failure.file_name.clone().unwrap_or_default()],
            summary: format!("{}: {}", failure.category.as_str(), failure.raw_message),
            content: serde_json::to_string_pretty(failure).unwrap_or_else(|_| failure.raw_message.clone()),
            created_at: epoch_secs(),
            updated_at: epoch_secs(),
            access_count: 0,
            version: 1,
        };
        Self::insert(conn, &entry)
    }

    pub fn record_fix_strategy(
        conn: &Connection,
        error_category: &FailureCategory,
        description: &str,
        affected_files: &[String],
    ) -> AppResult<()> {
        let mut tags = vec![error_category.as_str().to_string()];
        tags.extend(affected_files.iter().cloned());
        let entry = KnowledgeEntry {
            id: format!("fix-{}-{}", error_category.as_str(), epoch_secs()),
            category: KnowledgeCategory::FixStrategy,
            tags,
            summary: format!("Fix for {}: {}", error_category.as_str(), description),
            content: format!("Category: {}\nDescription: {}\nAffected files: {:?}", error_category.as_str(), description, affected_files),
            created_at: epoch_secs(),
            updated_at: epoch_secs(),
            access_count: 0,
            version: 1,
        };
        Self::insert(conn, &entry)
    }

    pub fn find_similar_plans(conn: &Connection, task_description: &str, limit: usize) -> AppResult<Vec<KnowledgeEntry>> {
        Self::query_by_category(conn, &KnowledgeCategory::Plan, limit * 2).map(|entries| {
            entries.into_iter().filter(|e| {
                e.summary.to_lowercase().contains(&task_description.to_lowercase())
                    || e.tags.iter().any(|t| task_description.contains(t.as_str()))
            }).take(limit).collect()
        })
    }
}

// ── Helpers ──

fn parse_category(raw: &str) -> KnowledgeCategory {
    match raw {
        "architecture" => KnowledgeCategory::Architecture,
        "dependency" => KnowledgeCategory::Dependency,
        "module" => KnowledgeCategory::Module,
        "symbol" => KnowledgeCategory::Symbol,
        "plan" => KnowledgeCategory::Plan,
        "fix_strategy" => KnowledgeCategory::FixStrategy,
        "error" => KnowledgeCategory::Error,
        "file" => KnowledgeCategory::File,
        "risk" => KnowledgeCategory::Risk,
        _ => KnowledgeCategory::Unknown,
    }
}

fn epoch_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_conn() -> Connection {
        let mut d = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        d.push(format!("nf_ks_test_{nanos}.db"));
        let conn = Connection::open(&d).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL").ok();
        KnowledgeStore::ensure_schema(&conn).unwrap();
        conn
    }

    #[test] fn insert_and_retrieve() {
        let conn = temp_conn();
        KnowledgeStore::insert(&conn, &KnowledgeEntry { id: "t1".into(), category: KnowledgeCategory::Architecture, tags: vec!["rust".into()], summary: "Arch".into(), content: "Tauri".into(), created_at: 0, updated_at: 0, access_count: 0, version: 1 }).unwrap();
        let r = KnowledgeStore::get_by_id(&conn, "t1").unwrap();
        assert_eq!(r.summary, "Arch");
    }

    #[test] fn upsert_merges() {
        let conn = temp_conn();
        KnowledgeStore::insert(&conn, &KnowledgeEntry { id: "m1".into(), category: KnowledgeCategory::Plan, tags: vec![], summary: "Plan".into(), content: "Step1".into(), created_at: 0, updated_at: 0, access_count: 0, version: 1 }).unwrap();
        KnowledgeStore::upsert(&conn, &KnowledgeEntry { id: "m1".into(), category: KnowledgeCategory::Plan, tags: vec![], summary: "Plan v2".into(), content: "Step2".into(), created_at: 0, updated_at: 0, access_count: 0, version: 2 }).unwrap();
        let r = KnowledgeStore::get_by_id(&conn, "m1").unwrap();
        assert!(r.content.contains("Step1") && r.content.contains("Step2"));
        assert!(r.version >= 2);
    }

    #[test] fn search_finds() {
        let conn = temp_conn();
        for i in 0..3 {
            KnowledgeStore::insert(&conn, &KnowledgeEntry { id: format!("s{}", i), category: KnowledgeCategory::Module, tags: vec![], summary: format!("mod{}", i), content: "x".into(), created_at: 0, updated_at: 0, access_count: 1, version: 1 }).unwrap();
        }
        KnowledgeStore::insert(&conn, &KnowledgeEntry { id: "auth".into(), category: KnowledgeCategory::Module, tags: vec![], summary: "authentication module".into(), content: "auth".into(), created_at: 0, updated_at: 0, access_count: 1, version: 1 }).unwrap();
        let r = KnowledgeStore::search(&conn, "authentication", 10).unwrap();
        assert_eq!(r.len(), 1);
    }

    #[test] fn expire_removes_stale() {
        let conn = temp_conn();
        KnowledgeStore::insert(&conn, &KnowledgeEntry { id: "old".into(), category: KnowledgeCategory::Error, tags: vec![], summary: "old".into(), content: "x".into(), created_at: 0, updated_at: 0, access_count: 0, version: 1 }).unwrap();
        KnowledgeStore::insert(&conn, &KnowledgeEntry { id: "fresh".into(), category: KnowledgeCategory::Error, tags: vec![], summary: "fresh".into(), content: "x".into(), created_at: 0, updated_at: 0, access_count: 100, version: 1 }).unwrap();
        conn.execute("UPDATE knowledge_entries SET updated_at = 0 WHERE id = 'old'", []).unwrap();
        let d = KnowledgeStore::expire_stale(&conn, 3600, 1).unwrap();
        assert!(d >= 1);
        assert!(KnowledgeStore::get_by_id(&conn, "old").is_err());
        assert!(KnowledgeStore::get_by_id(&conn, "fresh").is_ok());
    }

    #[test] fn dedup_merges() {
        let conn = temp_conn();
        for i in 0..3 {
            KnowledgeStore::insert(&conn, &KnowledgeEntry { id: format!("d{}", i), category: KnowledgeCategory::Error, tags: vec![], summary: "dup".into(), content: format!("c{}", i), created_at: 0, updated_at: 0, access_count: 0, version: 1 }).unwrap();
        }
        let m = KnowledgeStore::deduplicate(&conn).unwrap();
        assert!(m >= 1);
        let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM knowledge_entries WHERE summary='dup'", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 1);
    }

    #[test] fn record_plan_and_find() {
        let conn = temp_conn();
        let plan = TaskPlan { task_description: "Fix auth".into(), objective: "f".into(), affected_files: vec!["a.rs".into()], subtasks: vec![], risks: vec![], verification: vec![], unknown_information: vec![], confidence: 0.0, estimated_runtime_commands: 0, rollback_plan: String::new(), reasoning: String::new() };
        KnowledgeStore::record_plan(&conn, &plan).unwrap();
        let r = KnowledgeStore::find_similar_plans(&conn, "auth", 5).unwrap();
        assert!(!r.is_empty());
    }

    #[test] fn record_fix_and_retrieve() {
        let conn = temp_conn();
        KnowledgeStore::record_fix_strategy(&conn, &FailureCategory::MissingDependency, "npm install", &["package.json".into()]).unwrap();
        let r = KnowledgeStore::find_fix_strategies(&conn, &FailureCategory::MissingDependency, 5).unwrap();
        assert!(!r.is_empty());
    }
}