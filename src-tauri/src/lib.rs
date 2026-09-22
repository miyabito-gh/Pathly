use rusqlite::{params, Connection, DatabaseName, Result as SqlResult, Transaction};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

const KIND_FILE: &str = "file";
const KIND_FOLDER: &str = "folder";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredPath {
    pub id: i64,
    pub name: String,
    pub actual_name: String,
    pub path: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub memo: String,
    pub favorite: bool,
    pub use_count: i64,
    pub last_used_at: Option<String>,
    pub excluded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrokenPath {
    pub id: i64,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Taxonomy {
    pub tags: Vec<String>,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathValidation {
    pub input_path: String,
    pub normalized_path: String,
    pub actual_name: String,
    pub status: String,
    pub detected_kind: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordWrite {
    pub id: Option<i64>,
    pub name: String,
    pub path: String,
    pub kind_hint: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub category: Option<String>,
    #[serde(default)]
    pub memo: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub use_count: i64,
    pub last_used_at: Option<String>,
    #[serde(default)]
    pub excluded: bool,
}

#[derive(Debug, Clone)]
struct PreparedRecordWrite {
    id: Option<i64>,
    name: String,
    actual_name: String,
    path: String,
    kind: String,
    tags: Vec<String>,
    category: Option<String>,
    memo: String,
    favorite: bool,
    use_count: i64,
    last_used_at: Option<String>,
    excluded: bool,
}

pub trait PathRepository: Send {
    fn list(&self) -> SqlResult<Vec<RegisteredPath>>;
}

pub struct SqlitePathRepository {
    connection: Mutex<Connection>,
}

impl SqlitePathRepository {
    pub fn open(path: PathBuf) -> SqlResult<Self> {
        let connection = Connection::open(&path)?;
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
                excluded INTEGER NOT NULL DEFAULT 0,
                category TEXT
            );
            CREATE TABLE IF NOT EXISTS tags (
                path_id INTEGER NOT NULL REFERENCES registered_paths(id) ON DELETE CASCADE,
                value TEXT NOT NULL,
                PRIMARY KEY (path_id, value)
            );",
        )?;
        let has_category = {
            let mut statement = connection.prepare("PRAGMA table_info(registered_paths)")?;
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
            columns
                .collect::<SqlResult<Vec<_>>>()?
                .iter()
                .any(|name| name == "category")
        };
        if !has_category {
            connection.execute("ALTER TABLE registered_paths ADD COLUMN category TEXT", [])?;
        }
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn backup_to(&self, path: &PathBuf) -> SqlResult<()> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection.backup(DatabaseName::Main, path, None)
    }
}

impl PathRepository for SqlitePathRepository {
    fn list(&self) -> SqlResult<Vec<RegisteredPath>> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT id, name, actual_name, path, kind, memo, favorite, use_count, last_used_at, excluded, category
             FROM registered_paths ORDER BY use_count DESC, name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let mut tags =
                connection.prepare("SELECT value FROM tags WHERE path_id = ? ORDER BY value")?;
            let tags = tags
                .query_map(params![id], |tag| tag.get(0))?
                .collect::<SqlResult<Vec<String>>>()?;
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
                category: row.get(10)?,
            })
        })?;
        rows.collect()
    }
}

impl SqlitePathRepository {
    #[cfg(test)]
    fn register_with_metadata(
        &self,
        path: &std::path::Path,
        name: Option<String>,
        tags: Vec<String>,
        category: Option<String>,
        memo: String,
        favorite: bool,
        excluded: bool,
    ) -> SqlResult<RegisteredPath> {
        let metadata = std::fs::metadata(path)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let actual_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let name = name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| actual_name.clone());
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO registered_paths (name, actual_name, path, kind, memo, favorite, use_count, excluded, category) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8)",
            params![name, actual_name, path.to_string_lossy(), if metadata.is_dir() { "folder" } else { "file" }, memo, favorite, excluded, category.filter(|value| !value.trim().is_empty())],
        )?;
        let id = transaction.last_insert_rowid();
        for tag in tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
        {
            transaction.execute(
                "INSERT OR IGNORE INTO tags (path_id, value) VALUES (?1, ?2)",
                params![id, tag],
            )?;
        }
        transaction.commit()?;
        drop(connection);
        self.get(id)
    }

    fn get(&self, id: i64) -> SqlResult<RegisteredPath> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let mut statement = connection.prepare("SELECT id, name, actual_name, path, kind, memo, favorite, use_count, last_used_at, excluded, category FROM registered_paths WHERE id = ?1")?;
        statement.query_row(params![id], |row| {
            let id: i64 = row.get(0)?;
            let mut tag_statement =
                connection.prepare("SELECT value FROM tags WHERE path_id = ?1 ORDER BY value")?;
            let tags = tag_statement
                .query_map(params![id], |tag| tag.get(0))?
                .collect::<SqlResult<Vec<String>>>()?;
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
                category: row.get(10)?,
                tags,
            })
        })
    }

    fn write_record(transaction: &Transaction<'_>, record: &PreparedRecordWrite) -> SqlResult<i64> {
        let id = if let Some(id) = record.id {
            let changed = transaction.execute(
                "UPDATE registered_paths SET name = ?1, actual_name = ?2, path = ?3, kind = ?4, category = ?5, memo = ?6, favorite = ?7, use_count = ?8, last_used_at = ?9, excluded = ?10 WHERE id = ?11",
                params![record.name, record.actual_name, record.path, record.kind, record.category, record.memo, record.favorite, record.use_count, record.last_used_at, record.excluded, id],
            )?;
            if changed == 0 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            transaction.execute("DELETE FROM tags WHERE path_id = ?1", params![id])?;
            id
        } else {
            transaction.execute(
                "INSERT INTO registered_paths (name, actual_name, path, kind, category, memo, favorite, use_count, last_used_at, excluded) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![record.name, record.actual_name, record.path, record.kind, record.category, record.memo, record.favorite, record.use_count, record.last_used_at, record.excluded],
            )?;
            transaction.last_insert_rowid()
        };
        for tag in &record.tags {
            transaction.execute(
                "INSERT OR IGNORE INTO tags (path_id, value) VALUES (?1, ?2)",
                params![id, tag],
            )?;
        }
        Ok(id)
    }

    fn apply_record_batch(
        &self,
        records: Vec<PreparedRecordWrite>,
    ) -> SqlResult<Vec<RegisteredPath>> {
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        let mut ids = Vec::with_capacity(records.len());
        for record in &records {
            ids.push(Self::write_record(&transaction, record)?);
        }
        transaction.commit()?;
        drop(connection);
        ids.into_iter().map(|id| self.get(id)).collect()
    }

    #[cfg(test)]
    fn update(
        &self,
        id: i64,
        path: &Path,
        name: String,
        tags: Vec<String>,
        category: Option<String>,
        memo: String,
        favorite: bool,
        excluded: bool,
    ) -> SqlResult<RegisteredPath> {
        let metadata = std::fs::metadata(path)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let actual_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute("UPDATE registered_paths SET name = ?1, actual_name = ?2, path = ?3, kind = ?4, category = ?5, memo = ?6, favorite = ?7, excluded = ?8 WHERE id = ?9", params![name.trim(), actual_name, path.to_string_lossy(), if metadata.is_dir() { "folder" } else { "file" }, category.filter(|value| !value.trim().is_empty()), memo, favorite, excluded, id])?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        transaction.execute("DELETE FROM tags WHERE path_id = ?1", params![id])?;
        for tag in tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
        {
            transaction.execute(
                "INSERT OR IGNORE INTO tags (path_id, value) VALUES (?1, ?2)",
                params![id, tag],
            )?;
        }
        transaction.commit()?;
        drop(connection);
        self.get(id)
    }

    fn delete(&self, id: i64) -> SqlResult<()> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        if connection.execute("DELETE FROM registered_paths WHERE id = ?1", params![id])? == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    fn delete_many(&self, ids: &[i64]) -> SqlResult<()> {
        if ids.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "ids must not be empty".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        for id in ids {
            if transaction.execute("DELETE FROM registered_paths WHERE id = ?1", params![id])? == 0
            {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }
        transaction.commit()
    }

    fn path_for(&self, id: i64) -> SqlResult<PathBuf> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection
            .query_row(
                "SELECT path FROM registered_paths WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .map(PathBuf::from)
    }

    fn mark_used(&self, id: i64) -> SqlResult<()> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        if connection.execute("UPDATE registered_paths SET use_count = use_count + 1, last_used_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1", params![id])? == 0 { return Err(rusqlite::Error::QueryReturnedNoRows); }
        Ok(())
    }

    fn check_registered_paths(&self) -> SqlResult<Vec<BrokenPath>> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let mut statement =
            connection.prepare("SELECT id, name, path FROM registered_paths ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            Ok(BrokenPath {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
            })
        })?;
        let mut broken = Vec::new();
        for row in rows {
            let item = row?;
            if !Path::new(&item.path).exists() {
                broken.push(item);
            }
        }
        Ok(broken)
    }

    fn list_taxonomy(&self) -> SqlResult<Taxonomy> {
        let connection = self.connection.lock().expect("repository mutex poisoned");
        let mut tags_statement =
            connection.prepare("SELECT DISTINCT value FROM tags ORDER BY value ASC")?;
        let tags = tags_statement
            .query_map([], |row| row.get(0))?
            .collect::<SqlResult<Vec<String>>>()?;
        let mut categories_statement = connection.prepare("SELECT DISTINCT category FROM registered_paths WHERE category IS NOT NULL AND category <> '' ORDER BY category ASC")?;
        let categories = categories_statement
            .query_map([], |row| row.get(0))?
            .collect::<SqlResult<Vec<String>>>()?;
        Ok(Taxonomy { tags, categories })
    }

    fn rename_tag(&self, old_tag: &str, new_tag: &str) -> SqlResult<()> {
        if old_tag.trim().is_empty() || new_tag.trim().is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "タグ名は空にできません".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("repository mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO tags (path_id, value) SELECT path_id, ?1 FROM tags WHERE value = ?2",
            params![new_tag, old_tag],
        )?;
        transaction.execute("DELETE FROM tags WHERE value = ?1", params![old_tag])?;
        transaction.commit()
    }

    fn rename_category(&self, old_category: &str, new_category: &str) -> SqlResult<()> {
        if old_category.trim().is_empty() || new_category.trim().is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "カテゴリ名は空にできません".into(),
            ));
        }
        let connection = self.connection.lock().expect("repository mutex poisoned");
        connection.execute(
            "UPDATE registered_paths SET category = ?1 WHERE category = ?2",
            params![new_category, old_category],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
struct StorageSettings {
    directory: String,
    default_directory: String,
    is_custom: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedStorageSettings {
    directory: Option<PathBuf>,
}

struct StorageState {
    repository: SqlitePathRepository,
    current_directory: PathBuf,
    default_directory: PathBuf,
    settings_path: PathBuf,
    is_custom: bool,
}

struct AppState {
    storage: Mutex<StorageState>,
}

#[tauri::command]
fn list_registered_paths(state: State<'_, AppState>) -> Result<Vec<RegisteredPath>, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?;
    storage.repository.list().map_err(|error| error.to_string())
}

#[tauri::command]
fn check_registered_paths(state: State<'_, AppState>) -> Result<Vec<BrokenPath>, String> {
    state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .check_registered_paths()
        .map_err(|error| format!("リンク切れを確認できません: {error}"))
}

#[tauri::command]
fn list_taxonomy(state: State<'_, AppState>) -> Result<Taxonomy, String> {
    state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .list_taxonomy()
        .map_err(|error| format!("タグとカテゴリを取得できません: {error}"))
}

#[tauri::command]
fn rename_tag(old_tag: String, new_tag: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .rename_tag(&old_tag, &new_tag)
        .map_err(|error| format!("タグを変更できません: {error}"))
}

#[tauri::command]
fn rename_category(
    old_category: String,
    new_category: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .rename_category(&old_category, &new_category)
        .map_err(|error| format!("カテゴリを変更できません: {error}"))
}

#[tauri::command]
fn register_path(
    path: String,
    name: Option<String>,
    kind_hint: Option<String>,
    tags: Vec<String>,
    category: Option<String>,
    memo: String,
    favorite: bool,
    excluded: bool,
    state: State<'_, AppState>,
) -> Result<RegisteredPath, String> {
    let validation = validate_path_value(&path)?;
    let record = prepare_record_write(
        RecordWrite {
            id: None,
            name: name.unwrap_or_default(),
            path,
            kind_hint,
            tags,
            category,
            memo,
            favorite,
            use_count: 0,
            last_used_at: None,
            excluded,
        },
        validation,
        None,
    )?;
    state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .apply_record_batch(vec![record])
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "登録結果を取得できません".to_string())
}

fn normalize_input_path(path: &str) -> String {
    if let Some(unc_path) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc_path}")
    } else if let Some(drive_path) = path.strip_prefix(r"\\?\") {
        drive_path.to_string()
    } else {
        path.to_string()
    }
}

fn normalized_path_key(path: &str) -> String {
    normalize_input_path(path).replace('/', "\\").to_lowercase()
}

fn validate_kind_hint(kind_hint: Option<String>) -> Result<Option<String>, String> {
    match kind_hint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None => Ok(None),
        Some(KIND_FILE) => Ok(Some(KIND_FILE.to_string())),
        Some(KIND_FOLDER) => Ok(Some(KIND_FOLDER.to_string())),
        Some(_) => Err("種別は file または folder を指定してください".to_string()),
    }
}

fn validate_path_value(input: &str) -> Result<PathValidation, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("パスを入力してください".to_string());
    }
    let input_path = normalize_input_path(input);
    validate_path_syntax(&input_path)?;
    let path = PathBuf::from(&input_path);
    if !path.is_absolute() {
        return Err("登録するパスには絶対パスを指定してください".to_string());
    }

    let (normalized_path, status, detected_kind, message) = match std::fs::metadata(&path) {
        Ok(metadata) => {
            let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            (
                normalize_input_path(&canonical.to_string_lossy()),
                "exists".to_string(),
                Some(
                    if metadata.is_dir() {
                        KIND_FOLDER
                    } else {
                        KIND_FILE
                    }
                    .to_string(),
                ),
                "パスを確認できました。".to_string(),
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            input_path.clone(),
            "missing".to_string(),
            None,
            "パスは現在存在しません。種別を指定すれば登録できます。".to_string(),
        ),
        Err(error) => (
            input_path.clone(),
            "unavailable".to_string(),
            None,
            format!("パスを確認できませんでした。種別を指定すれば登録できます: {error}"),
        ),
    };
    let normalized = PathBuf::from(&normalized_path);
    let actual_name = normalized
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| normalized_path.clone());
    Ok(PathValidation {
        input_path: input.to_string(),
        normalized_path,
        actual_name,
        status,
        detected_kind,
        message,
    })
}

fn validate_path_syntax(path: &str) -> Result<(), String> {
    if path
        .chars()
        .any(|character| matches!(character, '\0' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err("パスに使用できない文字が含まれています".to_string());
    }
    for (index, _) in path
        .char_indices()
        .filter(|(_, character)| *character == ':')
    {
        let drive_colon =
            index == 1 && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
        if !drive_colon {
            return Err("パスのコロンの位置が正しくありません".to_string());
        }
    }
    if path.starts_with(r"\\") {
        let mut parts = path[2..].split(['\\', '/']).filter(|part| !part.is_empty());
        if parts.next().is_none() || parts.next().is_none() {
            return Err("UNCパスにはサーバー名と共有名を指定してください".to_string());
        }
    }
    Ok(())
}

fn prepare_record_write(
    record: RecordWrite,
    validation: PathValidation,
    existing: Option<&RegisteredPath>,
) -> Result<PreparedRecordWrite, String> {
    if record.use_count < 0 {
        return Err("利用回数は0以上で指定してください".to_string());
    }
    let kind_hint = validate_kind_hint(record.kind_hint)?;
    let unchanged_kind = existing
        .filter(|item| {
            normalized_path_key(&item.path) == normalized_path_key(&validation.normalized_path)
        })
        .map(|item| item.kind.clone());
    let kind = validation
        .detected_kind
        .clone()
        .or(kind_hint)
        .or(unchanged_kind)
        .ok_or_else(|| {
            "存在を確認できないパスでは、ファイルまたはフォルダーの種別を指定してください"
                .to_string()
        })?;
    let name = record.name.trim().to_string();
    let tags = record
        .tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(PreparedRecordWrite {
        id: record.id,
        name: if name.is_empty() {
            validation.actual_name.clone()
        } else {
            name
        },
        actual_name: validation.actual_name,
        path: validation.normalized_path,
        kind,
        tags,
        category: record
            .category
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        memo: record.memo,
        favorite: record.favorite,
        use_count: record.use_count,
        last_used_at: record
            .last_used_at
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        excluded: record.excluded,
    })
}

#[tauri::command]
fn validate_path(path: String, kind_hint: Option<String>) -> Result<PathValidation, String> {
    validate_kind_hint(kind_hint)?;
    validate_path_value(&path)
}

#[tauri::command]
fn update_registered_path(
    id: i64,
    path: String,
    name: String,
    kind_hint: Option<String>,
    tags: Vec<String>,
    category: Option<String>,
    memo: String,
    favorite: bool,
    excluded: bool,
    state: State<'_, AppState>,
) -> Result<RegisteredPath, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?;
    let existing = storage
        .repository
        .get(id)
        .map_err(|error| error.to_string())?;
    let validation = validate_path_value(&path)?;
    let record = prepare_record_write(
        RecordWrite {
            id: Some(id),
            name,
            path,
            kind_hint,
            tags,
            category,
            memo,
            favorite,
            use_count: existing.use_count,
            last_used_at: existing.last_used_at.clone(),
            excluded,
        },
        validation,
        Some(&existing),
    )?;
    storage
        .repository
        .apply_record_batch(vec![record])
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "更新結果を取得できません".to_string())
}

#[tauri::command]
fn apply_record_batch(
    records: Vec<RecordWrite>,
    state: State<'_, AppState>,
) -> Result<Vec<RegisteredPath>, String> {
    if records.is_empty() {
        return Err("反映するレコードがありません".to_string());
    }
    let storage = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?;
    let mut prepared = Vec::with_capacity(records.len());
    let mut seen_ids = std::collections::HashSet::new();
    for record in records {
        if let Some(id) = record.id {
            if !seen_ids.insert(id) {
                return Err(format!("同じID {id} が複数行に含まれています"));
            }
        }
        let existing = match record.id {
            Some(id) => Some(
                storage
                    .repository
                    .get(id)
                    .map_err(|_| format!("ID {id} のレコードが見つかりません"))?,
            ),
            None => None,
        };
        let validation = validate_path_value(&record.path)
            .map_err(|error| format!("{}: {error}", record.path))?;
        prepared.push(prepare_record_write(record, validation, existing.as_ref())?);
    }
    storage
        .repository
        .apply_record_batch(prepared)
        .map_err(|error| format!("レコードを反映できません: {error}"))
}

const RECORD_TEXT_LIMIT: u64 = 10 * 1024 * 1024;

fn validate_records_text_path(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(normalize_input_path(path.trim()));
    if !path.is_absolute() {
        return Err("CSVファイルには絶対パスを指定してください".to_string());
    }
    let supported = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"));
    if !supported {
        return Err("CSVファイル（.csv）を指定してください".to_string());
    }
    Ok(path)
}

#[tauri::command]
fn read_records_text_file(path: String) -> Result<String, String> {
    let path = validate_records_text_path(&path)?;
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("CSVファイルを確認できません: {error}"))?;
    if !metadata.is_file() {
        return Err("CSVファイルを指定してください".to_string());
    }
    if metadata.len() > RECORD_TEXT_LIMIT {
        return Err("CSVファイルは10MB以下にしてください".to_string());
    }
    let bytes =
        std::fs::read(&path).map_err(|error| format!("CSVファイルを読めません: {error}"))?;
    let text =
        String::from_utf8(bytes).map_err(|_| "CSVファイルはUTF-8で保存してください".to_string())?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

#[tauri::command]
fn write_records_text_file(path: String, content: String) -> Result<(), String> {
    let path = validate_records_text_path(&path)?;
    if content.len() as u64 > RECORD_TEXT_LIMIT {
        return Err("CSVデータは10MB以下にしてください".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "保存先フォルダーを確認できません".to_string())?;
    if !parent.is_dir() {
        return Err("存在する保存先フォルダーを指定してください".to_string());
    }
    std::fs::write(path, content.as_bytes())
        .map_err(|error| format!("CSVファイルを書き込めません: {error}"))
}

#[tauri::command]
fn delete_registered_path(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .delete(id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_registered_paths(ids: Vec<i64>, state: State<'_, AppState>) -> Result<(), String> {
    if ids.is_empty() {
        return Err("削除するレコードを選択してください".to_string());
    }
    let unique_ids: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    if unique_ids.len() != ids.len() || unique_ids.iter().any(|id| *id <= 0) {
        return Err("レコードIDが不正です".to_string());
    }
    state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .delete_many(&ids)
        .map_err(|error| format!("レコードを一括削除できません: {error}"))
}

#[tauri::command]
fn open_registered_path(id: i64, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let path = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .path_for(id)
        .map_err(|error| error.to_string())?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())?;
    if let Err(error) = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .mark_used(id)
    {
        eprintln!("Opened registered path {id}, but could not update usage history: {error}");
    }
    Ok(())
}

#[tauri::command]
async fn start_registered_path_drag(
    id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .path_for(id)
        .map_err(|error| error.to_string())?
        .canonicalize()
        .map_err(|error| format!("登録先を確認できません: {error}"))?;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "メインウィンドウを取得できません".to_string())?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let drag_window = window.clone();
    window
        .run_on_main_thread(move || {
            let result = drag::start_drag(
                &drag_window,
                drag::DragItem::Files(vec![path]),
                drag::Image::Raw(include_bytes!("../icons/icon.ico").to_vec()),
                |_result, _cursor_position| {},
                drag::Options::default(),
            )
            .map_err(|error| error.to_string());
            let _ = sender.send(result);
        })
        .map_err(|error| error.to_string())?;
    receiver.recv().map_err(|error| error.to_string())?
}

#[tauri::command]
fn open_registered_location(
    id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .repository
        .path_for(id)
        .map_err(|error| error.to_string())?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_storage_settings(state: State<'_, AppState>) -> Result<StorageSettings, String> {
    let storage = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?;
    Ok(storage.settings())
}

#[tauri::command]
fn set_storage_directory(
    directory: String,
    state: State<'_, AppState>,
) -> Result<StorageSettings, String> {
    let directory = PathBuf::from(directory.trim());
    if !directory.is_absolute() {
        return Err("保存先には絶対パスを指定してください".to_string());
    }
    switch_storage(&state, directory, true)
}

#[tauri::command]
fn reset_storage_directory(state: State<'_, AppState>) -> Result<StorageSettings, String> {
    let directory = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?
        .default_directory
        .clone();
    switch_storage(&state, directory, false)
}

impl StorageState {
    fn settings(&self) -> StorageSettings {
        StorageSettings {
            directory: self.current_directory.to_string_lossy().into_owned(),
            default_directory: self.default_directory.to_string_lossy().into_owned(),
            is_custom: self.is_custom,
        }
    }
}

fn switch_storage(
    state: &AppState,
    directory: PathBuf,
    is_custom: bool,
) -> Result<StorageSettings, String> {
    let mut storage = state
        .storage
        .lock()
        .map_err(|_| "保存先の状態を取得できません".to_string())?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("保存先フォルダーを作成できません: {error}"))?;
    let directory = std::fs::canonicalize(&directory)
        .map_err(|error| format!("保存先を確認できません: {error}"))?;
    let current = std::fs::canonicalize(&storage.current_directory)
        .unwrap_or_else(|_| storage.current_directory.clone());

    if directory == current {
        persist_storage_settings(
            &storage.settings_path,
            if is_custom { Some(&directory) } else { None },
        )?;
        storage.is_custom = is_custom;
        return Ok(storage.settings());
    }

    let target_database = directory.join("pathly.sqlite3");
    if target_database.exists() {
        return Err("選択したフォルダーには既にPathlyのデータベースがあります。データの上書きを避けるため、別のフォルダーを選んでください".to_string());
    }

    if let Err(error) = storage.repository.backup_to(&target_database) {
        let _ = std::fs::remove_file(&target_database);
        return Err(format!(
            "現在のデータを新しい保存先へ複製できません: {error}"
        ));
    }
    let next_repository = match SqlitePathRepository::open(target_database.clone()) {
        Ok(repository) => repository,
        Err(error) => {
            let _ = std::fs::remove_file(&target_database);
            return Err(format!("新しい保存先を開けません: {error}"));
        }
    };

    if let Err(error) = persist_storage_settings(
        &storage.settings_path,
        if is_custom { Some(&directory) } else { None },
    ) {
        drop(next_repository);
        let _ = std::fs::remove_file(&target_database);
        return Err(error);
    }

    storage.repository = next_repository;
    storage.current_directory = directory;
    storage.is_custom = is_custom;
    Ok(storage.settings())
}

fn persist_storage_settings(path: &PathBuf, directory: Option<&PathBuf>) -> Result<(), String> {
    let settings = PersistedStorageSettings {
        directory: directory.cloned(),
    };
    let bytes = serde_json::to_vec_pretty(&settings)
        .map_err(|error| format!("保存先設定を作成できません: {error}"))?;
    let temporary_path = path.with_extension("json.tmp");
    std::fs::write(&temporary_path, bytes)
        .map_err(|error| format!("保存先設定を書き込めません: {error}"))?;
    replace_settings_file(&temporary_path, path)
        .map_err(|error| format!("保存先設定を確定できません: {error}"))
}

#[cfg(windows)]
fn replace_settings_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR::from_raw(source.as_ptr()),
            PCWSTR::from_raw(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| std::io::Error::other(error.to_string()))
}

#[cfg(not(windows))]
fn replace_settings_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

fn default_storage_directory(
    is_debug: bool,
    manifest_directory: &PathBuf,
    executable: &PathBuf,
) -> PathBuf {
    if is_debug {
        manifest_directory
            .parent()
            .unwrap_or(manifest_directory)
            .to_path_buf()
    } else {
        executable.parent().unwrap_or(executable).to_path_buf()
    }
}

fn load_storage_state() -> Result<StorageState, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("実行ファイルの場所を取得できません: {error}"))?;
    let manifest_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let default_directory =
        default_storage_directory(cfg!(debug_assertions), &manifest_directory, &executable);
    let settings_directory = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| executable.parent().unwrap_or(&executable).to_path_buf())
        .join("Pathly");
    std::fs::create_dir_all(&settings_directory)
        .map_err(|error| format!("設定フォルダーを作成できません: {error}"))?;
    let settings_path = settings_directory.join("settings.json");
    let persisted = std::fs::read(&settings_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PersistedStorageSettings>(&bytes).ok())
        .unwrap_or_default();
    let is_custom = persisted.directory.is_some();
    let current_directory = persisted
        .directory
        .unwrap_or_else(|| default_directory.clone());
    std::fs::create_dir_all(&current_directory)
        .map_err(|error| format!("データ保存先を作成できません: {error}"))?;
    let repository = SqlitePathRepository::open(current_directory.join("pathly.sqlite3"))
        .map_err(|error| format!("Pathlyデータベースを開けません: {error}"))?;

    Ok(StorageState {
        repository,
        current_directory,
        default_directory,
        settings_path,
        is_custom,
    })
}

pub fn run() {
    let storage = load_storage_state().expect("failed to initialize Pathly storage");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            storage: Mutex::new(storage),
        })
        .invoke_handler(tauri::generate_handler![
            list_registered_paths,
            check_registered_paths,
            list_taxonomy,
            rename_tag,
            rename_category,
            register_path,
            update_registered_path,
            validate_path,
            apply_record_batch,
            read_records_text_file,
            write_records_text_file,
            delete_registered_path,
            delete_registered_paths,
            open_registered_path,
            start_registered_path_drag,
            open_registered_location,
            get_storage_settings,
            set_storage_directory,
            reset_storage_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pathly");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_replacement_preserves_new_value_over_existing_file() {
        let root = std::env::temp_dir().join(format!("pathly-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let settings_path = root.join("settings.json");
        let directory = root.join("custom");
        persist_storage_settings(&settings_path, Some(&directory)).unwrap();
        persist_storage_settings(&settings_path, None).unwrap();
        let saved: PersistedStorageSettings =
            serde_json::from_slice(&std::fs::read(&settings_path).unwrap()).unwrap();
        assert!(saved.directory.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_taxonomy_updates_all_items_without_duplicate_tags() {
        let root = std::env::temp_dir().join(format!("pathly-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let repository = SqlitePathRepository::open(root.join("db.sqlite3")).unwrap();
        let connection = repository.connection.lock().unwrap();
        connection.execute("INSERT INTO registered_paths (name, actual_name, path, kind, category) VALUES ('a','a','a','file','OldCat'), ('b','b','b','file','OldCat')", []).unwrap();
        connection
            .execute(
                "INSERT INTO tags (path_id, value) VALUES (1, 'old'), (1, 'new'), (2, 'old')",
                [],
            )
            .unwrap();
        drop(connection);
        repository.rename_tag("old", "new").unwrap();
        repository.rename_category("OldCat", "NewCat").unwrap();
        let taxonomy = repository.list_taxonomy().unwrap();
        assert_eq!(taxonomy.tags, vec!["new"]);
        assert_eq!(taxonomy.categories, vec!["NewCat"]);
        assert_eq!(
            repository
                .list()
                .unwrap()
                .iter()
                .map(|item| item.tags.len())
                .sum::<usize>(),
            2
        );
        assert!(repository.rename_tag(" ", "x").is_err());
        assert!(repository.rename_category("x", "").is_err());
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_link_check_returns_missing_registered_path() {
        let root = std::env::temp_dir().join(format!("pathly-broken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let repository = SqlitePathRepository::open(root.join("db.sqlite3")).unwrap();
        repository.connection.lock().unwrap().execute("INSERT INTO registered_paths (name, actual_name, path, kind) VALUES ('Missing item','missing.txt',?1,'file')", params![root.join("missing.txt").to_string_lossy()]).unwrap();
        let broken = repository.check_registered_paths().unwrap();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].id, 1);
        assert_eq!(broken[0].name, "Missing item");
        assert_eq!(broken[0].path, root.join("missing.txt").to_string_lossy());
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn development_default_is_repository_root() {
        let manifest = PathBuf::from("C:/work/Pathly/src-tauri");
        let executable = PathBuf::from("C:/somewhere/Pathly.exe");
        assert_eq!(
            default_storage_directory(true, &manifest, &executable),
            PathBuf::from("C:/work/Pathly")
        );
    }

    #[test]
    fn packaged_default_is_executable_directory() {
        let manifest = PathBuf::from("C:/work/Pathly/src-tauri");
        let executable = PathBuf::from("C:/Apps/Pathly/Pathly.exe");
        assert_eq!(
            default_storage_directory(false, &manifest, &executable),
            PathBuf::from("C:/Apps/Pathly")
        );
    }

    #[test]
    fn database_backup_preserves_registered_rows() {
        let directory =
            std::env::temp_dir().join(format!("pathly-storage-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let source = SqlitePathRepository::open(directory.join("source.sqlite3")).unwrap();
        source.connection.lock().unwrap().execute(
            "INSERT INTO registered_paths (id, name, actual_name, path, kind) VALUES (1, 'Test', 'test.txt', 'C:/test.txt', 'file')",
            [],
        ).unwrap();
        let destination_path = directory.join("destination.sqlite3");
        source.backup_to(&destination_path).unwrap();
        let destination = SqlitePathRepository::open(destination_path).unwrap();
        let items = destination.list().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Test");
        drop(destination);
        drop(source);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn lifecycle_persists_fields_and_lists_excluded_items() {
        let root = std::env::temp_dir().join(format!("pathly-lifecycle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("sample.txt");
        std::fs::write(&file, "sample").unwrap();
        let repository = SqlitePathRepository::open(root.join("db.sqlite3")).unwrap();
        let item = repository
            .register_with_metadata(&file, None, Vec::new(), None, String::new(), false, false)
            .unwrap();
        assert_eq!(item.name, "sample.txt");
        let updated = repository
            .update(
                item.id,
                &file,
                "Display".into(),
                vec!["tag".into()],
                Some("Work".into()),
                "memo".into(),
                true,
                true,
            )
            .unwrap();
        assert_eq!(updated.category.as_deref(), Some("Work"));
        assert!(updated.excluded);
        assert_eq!(updated.tags, vec!["tag"]);
        let listed = repository.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].excluded);
        repository.delete(item.id).unwrap();
        assert!(repository.list().unwrap().is_empty());
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bulk_delete_is_atomic_and_removes_only_database_records() {
        let root = std::env::temp_dir().join(format!("pathly-bulk-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let repository = SqlitePathRepository::open(root.join("db.sqlite3")).unwrap();
        repository
            .connection
            .lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO registered_paths (id, name, actual_name, path, kind) VALUES
                (1, 'First', 'first.txt', 'missing/first.txt', 'file'),
                (2, 'Second', 'second.txt', 'missing/second.txt', 'file'),
                (3, 'Keep', 'keep.txt', 'missing/keep.txt', 'file');
             INSERT INTO tags (path_id, value) VALUES (1, 'tag-one'), (2, 'tag-two');",
            )
            .unwrap();

        assert!(repository.delete_many(&[1, 2, 999]).is_err());
        assert_eq!(
            repository.list().unwrap().len(),
            3,
            "failed batch must rollback all deletions"
        );
        assert_eq!(
            repository
                .connection
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );

        repository.delete_many(&[1, 2]).unwrap();
        let remaining = repository.list().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, 3);
        assert_eq!(
            repository
                .connection
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn windows_extended_path_prefixes_are_normalized() {
        assert_eq!(
            normalize_input_path(r"\\?\C:\Users\sample\report.txt"),
            r"C:\Users\sample\report.txt"
        );
        assert_eq!(
            normalize_input_path(r"\\?\UNC\server\share\report.txt"),
            r"\\server\share\report.txt"
        );
        assert_eq!(
            normalize_input_path(r"\\?\C:\Users\sample\report.txt"),
            r"C:\Users\sample\report.txt"
        );
        assert_eq!(
            normalize_input_path(r"C:\Users\sample\report.txt"),
            r"C:\Users\sample\report.txt"
        );
    }

    #[test]
    fn path_validation_rejects_invalid_characters() {
        assert!(validate_path_syntax(r"C:\reports\*.csv").is_err());
        assert!(validate_path_syntax(r"C:\reports\valid.csv").is_ok());
        assert!(validate_path_syntax(r"C:\reports:archive\file.csv").is_err());
        assert!(validate_path_syntax(r"\\server").is_err());
        assert!(validate_path_syntax(r"\\server\share\file.csv").is_ok());
    }

    #[test]
    fn csv_text_io_is_limited_to_utf8_csv_files() {
        let root = std::env::temp_dir().join(format!("pathly-csv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let csv = root.join("records.csv");
        write_records_text_file(
            csv.to_string_lossy().into_owned(),
            "\u{feff}id,name\n1,項目".into(),
        )
        .unwrap();
        assert_eq!(
            read_records_text_file(csv.to_string_lossy().into_owned()).unwrap(),
            "id,name\n1,項目"
        );
        assert!(write_records_text_file(
            root.join("records.txt").to_string_lossy().into_owned(),
            "x".into()
        )
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_registration_saves_metadata_in_one_repository_operation() {
        let root =
            std::env::temp_dir().join(format!("pathly-register-full-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("proposal.txt");
        std::fs::write(&file, "proposal").unwrap();
        let repository = SqlitePathRepository::open(root.join("db.sqlite3")).unwrap();
        let item = repository
            .register_with_metadata(
                &file,
                Some("Proposal".into()),
                vec!["work".into(), "draft".into()],
                Some("Planning".into()),
                "review next week".into(),
                true,
                true,
            )
            .unwrap();
        assert_eq!(item.name, "Proposal");
        assert_eq!(item.tags, vec!["draft", "work"]);
        assert_eq!(item.category.as_deref(), Some("Planning"));
        assert_eq!(item.memo, "review next week");
        assert!(item.favorite);
        assert!(item.excluded);
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn successful_open_usage_update_increments_count_and_timestamp() {
        let root = std::env::temp_dir().join(format!("pathly-open-id-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let repository = SqlitePathRepository::open(root.join("db.sqlite3")).unwrap();
        let path = root.join("missing.txt");
        repository.connection.lock().unwrap().execute("INSERT INTO registered_paths (name, actual_name, path, kind) VALUES ('missing', 'missing.txt', ?1, 'file')", params![path.to_string_lossy()]).unwrap();
        let id = repository.connection.lock().unwrap().last_insert_rowid();
        assert!(repository.mark_used(id).is_ok());
        let item = repository.get(id).unwrap();
        assert_eq!(item.use_count, 1);
        assert!(item.last_used_at.is_some());
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_absolute_path_is_valid_with_user_selected_kind() {
        let root =
            std::env::temp_dir().join(format!("pathly-missing-write-{}", std::process::id()));
        let missing = root.join("later").join("report.txt");
        let validation = validate_path_value(&missing.to_string_lossy()).unwrap();
        assert_eq!(validation.status, "missing");
        assert_eq!(validation.detected_kind, None);
        let prepared = prepare_record_write(
            RecordWrite {
                id: None,
                name: String::new(),
                path: missing.to_string_lossy().into_owned(),
                kind_hint: Some(KIND_FILE.into()),
                tags: vec![" later ".into()],
                category: None,
                memo: String::new(),
                favorite: false,
                use_count: 0,
                last_used_at: None,
                excluded: false,
            },
            validation,
            None,
        )
        .unwrap();
        assert_eq!(prepared.kind, KIND_FILE);
        assert_eq!(prepared.actual_name, "report.txt");
        assert_eq!(prepared.name, "report.txt");
    }

    #[test]
    fn existing_path_metadata_overrides_kind_hint() {
        let root = std::env::temp_dir().join(format!("pathly-kind-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let validation = validate_path_value(&root.to_string_lossy()).unwrap();
        let prepared = prepare_record_write(
            RecordWrite {
                id: None,
                name: "Folder".into(),
                path: root.to_string_lossy().into_owned(),
                kind_hint: Some(KIND_FILE.into()),
                tags: Vec::new(),
                category: None,
                memo: String::new(),
                favorite: false,
                use_count: 0,
                last_used_at: None,
                excluded: false,
            },
            validation,
            None,
        )
        .unwrap();
        assert_eq!(prepared.kind, KIND_FOLDER);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_write_rolls_back_every_row_when_an_update_fails() {
        let root =
            std::env::temp_dir().join(format!("pathly-batch-rollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let repository = SqlitePathRepository::open(root.join("db.sqlite3")).unwrap();
        let base = PreparedRecordWrite {
            id: None,
            name: "new".into(),
            actual_name: "new.txt".into(),
            path: root.join("new.txt").to_string_lossy().into_owned(),
            kind: KIND_FILE.into(),
            tags: vec!["tag".into()],
            category: None,
            memo: String::new(),
            favorite: false,
            use_count: 0,
            last_used_at: None,
            excluded: false,
        };
        let mut missing_update = base.clone();
        missing_update.id = Some(999);
        assert!(repository
            .apply_record_batch(vec![base, missing_update])
            .is_err());
        assert!(repository.list().unwrap().is_empty());
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unchanged_missing_path_keeps_existing_kind_without_hint() {
        let root = std::env::temp_dir().join(format!("pathly-broken-edit-{}", std::process::id()));
        let path = root.join("missing-folder");
        let existing = RegisteredPath {
            id: 7,
            name: "old".into(),
            actual_name: "missing-folder".into(),
            path: path.to_string_lossy().into_owned(),
            kind: KIND_FOLDER.into(),
            tags: Vec::new(),
            category: None,
            memo: String::new(),
            favorite: false,
            use_count: 1,
            last_used_at: None,
            excluded: false,
        };
        let prepared = prepare_record_write(
            RecordWrite {
                id: Some(7),
                name: "new".into(),
                path: path.to_string_lossy().into_owned(),
                kind_hint: None,
                tags: Vec::new(),
                category: None,
                memo: String::new(),
                favorite: false,
                use_count: 1,
                last_used_at: None,
                excluded: false,
            },
            validate_path_value(&path.to_string_lossy()).unwrap(),
            Some(&existing),
        )
        .unwrap();
        assert_eq!(prepared.kind, KIND_FOLDER);
    }

    #[test]
    fn older_database_gets_nullable_category_column() {
        let root = std::env::temp_dir().join(format!("pathly-migration-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("old.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE registered_paths (id INTEGER PRIMARY KEY, name TEXT NOT NULL, actual_name TEXT NOT NULL, path TEXT NOT NULL, kind TEXT NOT NULL, memo TEXT NOT NULL DEFAULT '', favorite INTEGER NOT NULL DEFAULT 0, use_count INTEGER NOT NULL DEFAULT 0, last_used_at TEXT, excluded INTEGER NOT NULL DEFAULT 0); CREATE TABLE tags (path_id INTEGER NOT NULL, value TEXT NOT NULL, PRIMARY KEY(path_id, value));").unwrap();
        drop(connection);
        let repository = SqlitePathRepository::open(path).unwrap();
        assert_eq!(repository.list().unwrap().len(), 0);
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changing_storage_copies_database_and_keeps_source() {
        let root = std::env::temp_dir().join(format!("pathly-switch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source_directory = root.join("project");
        let destination_directory = root.join("custom-data");
        std::fs::create_dir_all(&source_directory).unwrap();
        let repository =
            SqlitePathRepository::open(source_directory.join("pathly.sqlite3")).unwrap();
        repository.connection.lock().unwrap().execute(
            "INSERT INTO registered_paths (id, name, actual_name, path, kind) VALUES (1, 'Keep me', 'keep.txt', 'C:/keep.txt', 'file')",
            [],
        ).unwrap();
        let state = AppState {
            storage: Mutex::new(StorageState {
                repository,
                current_directory: source_directory.clone(),
                default_directory: source_directory.clone(),
                settings_path: root.join("settings.json"),
                is_custom: false,
            }),
        };

        let settings = switch_storage(&state, destination_directory.clone(), true).unwrap();
        assert!(settings.is_custom);
        assert_eq!(
            PathBuf::from(settings.directory),
            std::fs::canonicalize(&destination_directory).unwrap()
        );
        assert!(source_directory.join("pathly.sqlite3").exists());
        let storage = state.storage.lock().unwrap();
        assert_eq!(storage.repository.list().unwrap()[0].name, "Keep me");
        let persisted: PersistedStorageSettings =
            serde_json::from_slice(&std::fs::read(&storage.settings_path).unwrap()).unwrap();
        assert_eq!(
            persisted.directory,
            Some(std::fs::canonicalize(&destination_directory).unwrap())
        );
        drop(storage);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }
}
