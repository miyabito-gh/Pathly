use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredPath {
    pub id: i64,
    pub name: String,
    pub actual_name: String,
    pub path: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub memo: String,
    pub favorite: bool,
    pub use_count: i64,
    pub last_used_at: Option<String>,
    pub excluded: bool,
}

pub trait PathRepository: Send {
    fn list(&self) -> SqlResult<Vec<RegisteredPath>>;
}

pub struct SqlitePathRepository {
    connection: Mutex<Connection>,
}

impl SqlitePathRepository {
    pub fn open(path: PathBuf) -> SqlResult<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS registered_paths (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                actual_name TEXT NOT NULL,
                path TEXT NOT NULL,
                kind TEXT NOT NULL,
                memo TEXT NOT NULL DEFAULT '',
                favorite INTEGER NOT NULL DEFAULT 0,
                use_count INTEGER NOT NULL DEFAULT 0,
                last_used_at TEXT,
                excluded INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS tags (
                path_id INTEGER NOT NULL REFERENCES registered_paths(id) ON DELETE CASCADE,
                value TEXT NOT NULL,
                PRIMARY KEY (path_id, value)
            );",
        )?;
        Ok(Self { connection: Mutex::new(connection) })
    }
}

impl PathRepository for SqlitePathRepository {
    fn list(&self) -> SqlResult<Vec<RegisteredPath>> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT id, name, actual_name, path, kind, memo, favorite, use_count, last_used_at, excluded
             FROM registered_paths WHERE excluded = 0 ORDER BY use_count DESC, name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let mut tags = connection.prepare("SELECT value FROM tags WHERE path_id = ? ORDER BY value")?;
            let tags = tags.query_map(params![id], |tag| tag.get(0))?.collect::<SqlResult<Vec<String>>>()?;
            Ok(RegisteredPath {
                id,
                name: row.get(1)?,
                actual_name: row.get(2)?,
                path: row.get(3)?,
                kind: row.get(4)?,
                memo: row.get(5)?,
                favorite: row.get::<_, i64>(6)? != 0,
                use_count: row.get(7)?,
                last_used_at: row.get(8)?,
                excluded: row.get::<_, i64>(9)? != 0,
                tags,
            })
        })?;
        rows.collect()
    }
}

struct AppState {
    repository: SqlitePathRepository,
}

#[tauri::command]
fn list_registered_paths(state: State<'_, AppState>) -> Result<Vec<RegisteredPath>, String> {
    state.repository.list().map_err(|error| error.to_string())
}

pub fn run() {
    let data_dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Pathly");
    std::fs::create_dir_all(&data_dir).expect("failed to create Pathly data directory");
    let repository = SqlitePathRepository::open(data_dir.join("pathly.sqlite3")).expect("failed to open Pathly database");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { repository })
        .invoke_handler(tauri::generate_handler![list_registered_paths])
        .run(tauri::generate_context!())
        .expect("error while running Pathly");
}
