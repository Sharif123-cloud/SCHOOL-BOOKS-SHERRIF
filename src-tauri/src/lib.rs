use tauri::Manager;
use std::sync::Mutex;
use chrono::{Utc, Duration};
use serde::{Serialize, Deserialize};
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;

struct AppState {
    last_unlock: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
}

#[derive(Serialize, Deserialize)]
struct Book {
    id: i64,
    title: String,
    path: String,
    format: String,
    current_page: i32,
    total_pages: i32,
    progress: i32,
}

fn init_db(app_dir: PathBuf) -> Connection {
    let db_path = app_dir.join("reader.db");
    let conn = Connection::open(db_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS books (
            id INTEGER PRIMARY KEY,
            title TEXT,
            path TEXT,
            format TEXT,
            total_pages INTEGER,
            current_page INTEGER DEFAULT 1,
            last_opened TIMESTAMP
        )", [],
    ).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY,
            book_id INTEGER,
            page INTEGER,
            note TEXT
        )", [],
    ).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS notes (
            id INTEGER PRIMARY KEY,
            book_id INTEGER,
            page INTEGER,
            note TEXT
        )", [],
    ).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS highlights (
            id INTEGER PRIMARY KEY,
            book_id INTEGER,
            page INTEGER,
            text TEXT
        )", [],
    ).unwrap();
    conn
}

#[tauri::command]
fn needs_authentication(state: tauri::State<AppState>) -> bool {
    let last = state.last_unlock.lock().unwrap();
    match *last {
        None => true,
        Some(t) => Utc::now() - t > Duration::days(2),
    }
}

#[tauri::command]
fn verify_password(password: String, state: tauri::State<AppState>, app_handle: tauri::AppHandle) -> bool {
    let required = "SSERUNJOGISHARIF47@GMAIL.COM";
    if password != required { return false; }
    *state.last_unlock.lock().unwrap() = Some(Utc::now());
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let ts_file = app_dir.join("last_unlock");
    let _ = fs::write(ts_file, Utc::now().to_rfc3339());
    true
}

#[tauri::command]
fn get_library(app_handle: tauri::AppHandle) -> Vec<Book> {
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let conn = init_db(app_dir);
    let mut stmt = conn.prepare(
        "SELECT id, title, path, format, current_page, total_pages, (current_page * 100 / total_pages) as progress FROM books"
    ).unwrap();
    let books = stmt.query_map([], |row| {
        Ok(Book {
            id: row.get(0)?,
            title: row.get(1)?,
            path: row.get(2)?,
            format: row.get(3)?,
            current_page: row.get(4)?,
            total_pages: row.get(5)?,
            progress: row.get(6)?,
        })
    }).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
    books
}

#[tauri::command]
fn import_book(path: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    let src = Path::new(&path);
    if !src.exists() { return Err("File not found".into()); }
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let books_dir = app_dir.join("books");
    fs::create_dir_all(&books_dir).map_err(|e| e.to_string())?;
    let dest = books_dir.join(src.file_name().unwrap());
    fs::copy(src, &dest).map_err(|e| e.to_string())?;
    let format = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    let title = src.file_stem().unwrap().to_string_lossy().to_string();
    let total_pages = if format == "pdf" {
        count_pdf_pages(&dest)
    } else if format == "epub" {
        count_epub_pages(&dest)
    } else {
        1
    };
    let conn = init_db(app_dir);
    conn.execute(
        "INSERT INTO books (title, path, format, total_pages) VALUES (?1, ?2, ?3, ?4)",
        params![title, dest.to_str().unwrap(), format, total_pages],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

fn count_pdf_pages(path: &Path) -> i32 {
    if let Ok(doc) = lopdf::Document::load(path) {
        doc.get_pages().len() as i32
    } else { 1 }
}
fn count_epub_pages(path: &Path) -> i32 {
    if let Ok(epub) = epub::doc::EpubDoc::new(path.to_str().unwrap()) {
        epub.get_num_pages().unwrap_or(1) as i32
    } else { 1 }
}

#[tauri::command]
fn open_book(id: i64, app_handle: tauri::AppHandle) -> Result<Book, String> {
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let conn = init_db(app_dir);
    let mut stmt = conn.prepare("SELECT id, title, path, format, current_page, total_pages FROM books WHERE id = ?1").unwrap();
    let book = stmt.query_row(params![id], |row| {
        Ok(Book {
            id: row.get(0)?,
            title: row.get(1)?,
            path: row.get(2)?,
            format: row.get(3)?,
            current_page: row.get(4)?,
            total_pages: row.get(5)?,
            progress: 0,
        })
    }).map_err(|_| "Book not found")?;
    Ok(book)
}

#[tauri::command]
fn get_page_content(book_id: i64, page: i32, font_size: i32, font_family: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let conn = init_db(app_dir);
    let mut stmt = conn.prepare("SELECT path, format FROM books WHERE id = ?1").unwrap();
    let (path, format): (String, String) = stmt.query_row(params![book_id], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).map_err(|_| "Book not found")?;
    let content = if format == "pdf" {
        extract_pdf_page(&Path::new(&path), page)
    } else if format == "epub" {
        extract_epub_page(&Path::new(&path), page)
    } else {
        "Unsupported format".to_string()
    };
    let html = format!(r#"<div style="font-family: {}; font-size: {}px; line-height: 1.6;">{}</div>"#, font_family, font_size, content);
    Ok(html)
}

fn extract_pdf_page(path: &Path, page_num: i32) -> String {
    if let Ok(doc) = lopdf::Document::load(path) {
        let pages = doc.get_pages();
        if let Some(page_id) = pages.values().nth((page_num-1) as usize) {
            if let Ok(text) = doc.extract_text(&[*page_id]) {
                return text.replace('\n', "<br>");
            }
        }
    }
    "Could not extract text".to_string()
}
fn extract_epub_page(path: &Path, page_num: i32) -> String {
    if let Ok(mut epub) = epub::doc::EpubDoc::new(path.to_str().unwrap()) {
        if let Some(content) = epub.get_content_str(page_num as usize) {
            return content;
        }
    }
    "No content".to_string()
}

#[tauri::command]
fn save_progress(book_id: i64, page: i32, app_handle: tauri::AppHandle) {
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let conn = init_db(app_dir);
    let _ = conn.execute("UPDATE books SET current_page = ?1 WHERE id = ?2", params![page, book_id]);
}

#[tauri::command]
fn search_in_book(book_id: i64, query: String, app_handle: tauri::AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let conn = init_db(app_dir);
    let mut stmt = conn.prepare("SELECT path, format, total_pages FROM books WHERE id = ?1").unwrap();
    let (path, format, total_pages): (String, String, i32) = stmt.query_row(params![book_id], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }).unwrap();
    let mut results = vec![];
    for page in 1..=total_pages {
        let text = if format == "pdf" {
            extract_pdf_page(&Path::new(&path), page)
        } else {
            extract_epub_page(&Path::new(&path), page)
        };
        if text.to_lowercase().contains(&query.to_lowercase()) {
            results.push(serde_json::json!({ "page": page, "context": text.chars().take(200).collect::<String>() }));
        }
    }
    Ok(results)
}

#[tauri::command]
fn add_bookmark(book_id: i64, page: i32, note: String, app_handle: tauri::AppHandle) {
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let conn = init_db(app_dir);
    let _ = conn.execute("INSERT INTO bookmarks (book_id, page, note) VALUES (?1, ?2, ?3)", params![book_id, page, note]);
}

#[tauri::command]
fn add_note(book_id: i64, page: i32, note: String, app_handle: tauri::AppHandle) {
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let conn = init_db(app_dir);
    let _ = conn.execute("INSERT INTO notes (book_id, page, note) VALUES (?1, ?2, ?3)", params![book_id, page, note]);
}

#[tauri::command]
fn add_highlight(book_id: i64, page: i32, text: String, app_handle: tauri::AppHandle) {
    let app_dir = app_handle.path_resolver().app_data_dir().unwrap();
    let conn = init_db(app_dir);
    let _ = conn.execute("INSERT INTO highlights (book_id, page, text) VALUES (?1, ?2, ?3)", params![book_id, page, text]);
}

#[tauri::command]
fn scan_folder_for_books(folder: String) -> Result<Vec<String>, String> {
    let mut files = vec![];
    for entry in WalkDir::new(&folder).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if ext == "pdf" || ext == "epub" {
                files.push(path.file_name().unwrap().to_string_lossy().to_string());
            }
        }
    }
    Ok(files)
}

#[tauri::command]
fn batch_import(folder: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    for entry in WalkDir::new(&folder).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if ext == "pdf" || ext == "epub" {
                let _ = import_book(path.to_str().unwrap().to_string(), app_handle.clone());
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn get_reading_stats() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "totalMinutes": 42 }))
}

#[tauri::command]
fn lookup_word(word: String) -> Result<String, String> {
    Ok(format!("Definition of '{}' not available offline.", word))
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            last_unlock: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            needs_authentication,
            verify_password,
            get_library,
            import_book,
            open_book,
            get_page_content,
            save_progress,
            search_in_book,
            add_bookmark,
            add_note,
            add_highlight,
            scan_folder_for_books,
            batch_import,
            get_reading_stats,
            lookup_word,
        ])
        .setup(|app| {
            let app_dir = app.path_resolver().app_data_dir().unwrap();
            fs::create_dir_all(&app_dir).unwrap();
            let ts_file = app_dir.join("last_unlock");
            if ts_file.exists() {
                let ts_str = fs::read_to_string(ts_file).unwrap();
                if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&ts_str) {
                    *app.state::<AppState>().last_unlock.lock().unwrap() = Some(ts.with_timezone(&chrono::Utc));
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
