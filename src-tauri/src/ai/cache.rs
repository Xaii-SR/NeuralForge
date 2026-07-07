use crate::ai::providers::ollama::ChatMessage;
use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

fn hash_prompt(model: &str, messages: &[ChatMessage]) -> String {
    let mut hasher = DefaultHasher::new();
    model.hash(&mut hasher);
    for m in messages {
        m.role.hash(&mut hasher);
        m.content.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

pub fn get_cached(conn: &Connection, model: &str, messages: &[ChatMessage]) -> Option<String> {
    let hash = hash_prompt(model, messages);
    conn.query_row(
        "SELECT response FROM response_cache WHERE prompt_hash = ?1 AND model = ?2",
        params![hash, model],
        |row| row.get(0),
    )
    .ok()
}

pub fn store_response(conn: &Connection, model: &str, messages: &[ChatMessage], response: &str) -> AppResult<()> {
    let hash = hash_prompt(model, messages);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    conn.execute(
        "INSERT INTO response_cache (prompt_hash, model, response, success_rating, created_at)
         VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(prompt_hash, model) DO UPDATE SET response = excluded.response, created_at = excluded.created_at",
        params![hash, model, response, now],
    )
    .map_err(|e| AppError::Provider(format!("failed to cache response: {e}")))?;
    Ok(())
}

pub fn clear_cache(conn: &Connection) -> AppResult<usize> {
    conn.execute("DELETE FROM response_cache", [])
        .map_err(|e| AppError::Provider(format!("failed to clear cache: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(content: &str) -> Vec<ChatMessage> {
        vec![ChatMessage {
            role: "user".to_string(),
            content: content.to_string(),
        }]
    }

    #[test]
    fn cache_miss_then_hit_after_store() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_cache_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();

        let conn = crate::database::open_for_workspace(&dir).unwrap();

        assert!(get_cached(&conn, "model-a", &msgs("hello")).is_none());

        store_response(&conn, "model-a", &msgs("hello"), "hi there").unwrap();
        assert_eq!(get_cached(&conn, "model-a", &msgs("hello")), Some("hi there".to_string()));

        // Different model or different prompt -> still a miss
        assert!(get_cached(&conn, "model-b", &msgs("hello")).is_none());
        assert!(get_cached(&conn, "model-a", &msgs("goodbye")).is_none());

        let cleared = clear_cache(&conn).unwrap();
        assert_eq!(cleared, 1);
        assert!(get_cached(&conn, "model-a", &msgs("hello")).is_none());

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
