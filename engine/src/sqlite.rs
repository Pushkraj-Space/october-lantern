//! Read agent databases directly on every OS. No SQLite executable or URI escaping is needed.
use anyhow::Result;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Map, Value};
use std::path::Path;
use std::time::{Duration, Instant};

pub fn query(path: &Path, sql: &str) -> Result<Vec<Value>> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    connection.busy_timeout(Duration::from_millis(250))?;
    connection.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, 16 * 1024 * 1024)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    connection.progress_handler(10000, Some(move || Instant::now() > deadline))?;
    let mut statement = connection.prepare(sql)?;
    anyhow::ensure!(statement.readonly(), "Agent queries must be read-only");
    let columns: Vec<String> = statement.column_names().iter().map(|name| name.to_string()).collect();
    let mut rows = statement.query([])?;
    let mut result = Vec::new();
    let mut size = 0;
    while let Some(row) = rows.next()? {
        anyhow::ensure!(result.len() < 4096 && Instant::now() < deadline, "Agent database query exceeded its limit");
        let mut object = Map::new();
        for (index, name) in columns.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(value) => value.into(),
                ValueRef::Real(value) => serde_json::Number::from_f64(value).map(Value::Number).unwrap_or(Value::Null),
                ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                    size += bytes.len();
                    anyhow::ensure!(size <= 16 * 1024 * 1024, "Agent database response exceeds 16 MB");
                    String::from_utf8_lossy(bytes).into_owned().into()
                }
            };
            object.insert(name.clone(), value);
        }
        result.push(Value::Object(object));
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_live_wal_with_unusual_paths_and_refuses_writes() {
        // Characters a SQLite URI would misread. Windows doesn't allow `?` in file names.
        let odd = if cfg!(windows) { "#%&" } else { "#?" };
        let dir = std::env::temp_dir().join(format!("lantern SQLite {odd} {}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join(format!("agent {odd}.db"));
        let writer = Connection::open(&path).unwrap();
        writer
            .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE messages(id INTEGER, text TEXT); INSERT INTO messages VALUES(1,'hello');")
            .unwrap();
        let values = query(&path, "SELECT id,text,json_extract('{\"role\":\"user\"}', '$.role') AS role FROM messages").unwrap();
        assert_eq!(values[0], serde_json::json!({"id":1,"text":"hello","role":"user"}));
        assert!(query(&path, "DELETE FROM messages").is_err());
        assert_eq!(query(&path, "SELECT id FROM messages").unwrap().len(), 1);
        drop(writer);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn missing_databases_are_never_created() {
        let path = std::env::temp_dir().join(format!("missing-{}.db", uuid::Uuid::new_v4()));
        assert!(query(&path, "SELECT 1").is_err());
        assert!(!path.exists());
    }
}
