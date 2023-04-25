use md5::{Digest, Md5};
use reqwest::{blocking::Client, Url};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    backtrace::Backtrace,
    collections::HashMap,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::Path,
};
use thiserror::Error;
use url::ParseError;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("mutex error")]
    MutexError,
    #[error("io error: {source} at {backtrace}")]
    IOError {
        #[from]
        source: std::io::Error,
        backtrace: Backtrace,
    },
    #[error("sqlite error")]
    SqliteError {
        #[from]
        source: rusqlite::Error,
    },
    #[error("reqwest error")]
    ReqwestError {
        #[from]
        source: reqwest::Error,
    },
    #[error("serde json error")]
    SerdeJsonError {
        #[from]
        source: serde_json::Error,
    },
    #[error("url parse error")]
    UrlParseError {
        #[from]
        source: ParseError,
    },
    #[error("file path error, may exist non-UTF-8 strings")]
    FilePathToStrError,
    #[error("file download fail")]
    FileDownloadFail(String),
    #[error("ok_or error")]
    OkOrError(String),
}
pub type Result<T, E = ApplicationError> = std::result::Result<T, E>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    app_id: String,
    version: i32,
    force: bool,
    os: String,
    use_app_store: bool,
    app_uri: String,
    meta_info: HashMap<String, MetaValue>,
    entry: ModuleInfo,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaValue {
    content: String,
    value_type: MetaValueType,
    description: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum MetaValueType {
    #[serde(rename = "0")]
    TypeNumber,
    #[serde(rename = "1")]
    TypeString,
    #[serde(rename = "2")]
    TypeBool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    app_id: String,
    id: String,
    version: String,
    os: String,
    agent: String,
    script: Vec<Source>,
    ttf: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    src: String,
    r#async: bool,
}

const TABLE_NAME_APP_INFO: &str = "app_info";
const TABLE_NAME_RESOURCE: &str = "resource";

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse<T> {
    code: i32,
    data: T,
}

#[derive(Debug)]
pub struct Resource {
    id: i64,
    url: String,
    path: String,
    hash_code: String,
    cache_ctrl: String, // todo: cacheCtrl policy
}

#[derive(Debug)]
pub struct Context {
    db_path: String,
    cache_dir: String,
}
impl Context {
    pub fn new(db_path: &str, cache_dir: &str) -> Self {
        Context {
            db_path: db_path.to_string(),
            cache_dir: cache_dir.to_string(),
        }
    }

    pub fn get_app_info_str(&self, server: &str, id: &str) -> Result<Option<String>> {
        match self.get_app_info(server, id)? {
            Some(info) => Ok(Some(serde_json::to_string(&info)?)),
            None => Ok(None),
        }
    }

    pub fn get_app_info(&self, server: &str, id: &str) -> Result<Option<AppInfo>> {
        println!(
            "enter get_app_info, server: {:}, id: {:}, context: {:?}",
            server, id, self
        );
        let conn: Connection = Connection::open(&self.db_path)?;
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                     id TEXT PRIMARY KEY,
                     app_info BLOB
                 )",
                TABLE_NAME_APP_INFO
            ),
            params![],
        )?;

        let get_cached_app_info = || -> Result<Option<AppInfo>> {
            let result = conn
                .query_row(
                    &format!("SELECT app_info FROM {} WHERE id = ?", TABLE_NAME_APP_INFO),
                    params![id],
                    |row| row.get::<usize, String>(0),
                )
                .optional()?;
            match result {
                Some(raw) => {
                    let app_info = serde_json::from_str::<AppInfo>(&raw)?;
                    Ok(Some(app_info))
                }
                None => Ok(None),
            }
        };

        let client = Client::new();
        let url = format!("{}/appinfo/{}", server, id);
        println!("request url: {:}", url);
        let res = client.get(&url).send()?;

        if res.status().is_success() {
            let api_res: ApiResponse<AppInfo> = res.json()?;
            println!("api_res: {:?}", api_res);
            match api_res.code {
                0 => {
                    println!("cache appinfo");
                    conn.execute(
                        &format!(
                            "INSERT OR REPLACE INTO {} (id, app_info) VALUES (?, ?)",
                            TABLE_NAME_APP_INFO
                        ),
                        params![id, serde_json::to_vec(&api_res.data)?],
                    )?;
                    Ok(Some(api_res.data))
                }
                _ => get_cached_app_info(),
            }
        } else {
            println!("request fail: {:}", res.status());
            get_cached_app_info()
        }
    }

    pub fn get_resource(&self, url: &str, disable_cache: bool) -> Result<Option<String>> {
        println!(
            "get_resource url: {:}, disable_cache: {}, cache_dir: {}, db_path: {}",
            url, disable_cache, self.cache_dir, self.db_path
        );
        if !Path::new(&self.cache_dir).exists() {
            println!("path not exist, try to create");
            fs::create_dir_all(&self.cache_dir)?;
            println!("create dir: {}", &self.cache_dir);
        }
        let conn = Connection::open(&self.db_path)?;

        // create table for resources if not exists
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 url TEXT NOT NULL,
                 path TEXT NOT NULL,
                 hash_code TEXT NOT NULL,
                 cache_ctrl TEXT NOT NULL
             )",
                TABLE_NAME_RESOURCE
            ),
            params![],
        )?;

        let mut stmt = conn.prepare(&format!(
            "SELECT * FROM {} WHERE url = ?1",
            TABLE_NAME_RESOURCE
        ))?;
        let mut rows = stmt.query(params![url])?;

        let mut resource: Option<Resource> = None;

        if let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let url: String = row.get(1)?;
            let path: String = row.get(2)?;
            let hash_code: String = row.get(3)?;
            let cache_ctrl: String = row.get(4)?;

            println!(
                "find cache: {}, {}, {}, {}",
                url, path, hash_code, cache_ctrl
            );
            if Path::new(&path).exists() && !disable_cache {
                // checksum file
                let file_hash = md5_file(&path)?;
                if file_hash == hash_code {
                    resource = Some(Resource {
                        id,
                        url,
                        path,
                        hash_code,
                        cache_ctrl,
                    });
                }
            }
            // hash code not match or resource.path not exist, clean record
            conn.execute(
                &format!("DELETE FROM {} WHERE id = ?1", TABLE_NAME_RESOURCE),
                params![id],
            )?;
        }

        if resource.is_none() {
            let file_name = download(url, &self.cache_dir)?;
            let file_path = Path::new(&self.cache_dir).join(file_name);
            let hash_code = md5_file(&file_path)?;
            let cache_ctrl = ""; // todo: cacheCtrl policy
            let file_path = file_path.to_string_lossy();
            conn.execute(
                &format!(
                    "INSERT INTO {} (url, path, hash_code, cache_ctrl) VALUES (?1, ?2, ?3, ?4)",
                    TABLE_NAME_RESOURCE
                ),
                params![url, file_path, hash_code, cache_ctrl],
            )?;
            println!(
                "save cache: {}, {}, {}, {}",
                url, file_path, hash_code, cache_ctrl
            );

            resource = Some(Resource {
                id: conn.last_insert_rowid(),
                url: url.to_string(),
                path: file_path.to_string(),
                hash_code,
                cache_ctrl: cache_ctrl.to_string(),
            });
        }

        Ok(Some(resource.unwrap().path))
    }

    pub fn desotry(&mut self) -> Result<()> {
        if self.is_db_exist() {
            fs::remove_file(&self.db_path)?;
        }
        Ok(())
    }

    fn is_db_exist(&self) -> bool {
        let path = Path::new(&self.db_path);
        path.exists()
    }
}

fn download(raw_url: &str, folder: &str) -> Result<String> {
    let client = Client::new();

    let url = Url::parse(raw_url)?;
    let file_name = Uuid::new_v4().to_string();

    let file_path = Path::new(folder).join(&file_name);
    println!("new file name: {}", file_name);
    let mut file = fs::File::create(&file_path)?;

    let mut response = client.get(url).send()?;
    if !response.status().is_success() {
        return Err(ApplicationError::FileDownloadFail(raw_url.to_string()));
    }
    let mut buf: Vec<u8> = vec![];

    response.copy_to(&mut buf)?;
    file.write_all(&buf)?;

    Ok(file_name)
}

fn md5(content: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

fn md5_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Md5::new();
    loop {
        let length = {
            let buffer = reader.fill_buf()?;
            hasher.update(buffer);
            buffer.len()
        };

        if length == 0 {
            break;
        }

        reader.consume(length);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    const TEST_DB_PATH: &str = "test.db";
    const TEST_CACHE_DIR: &str = "test_cache";
    use httpmock::prelude::*;

    fn setup() {
        // Cleanup before running tests
        cleanup();
    }

    fn cleanup() {
        if Path::new(TEST_DB_PATH).exists() {
            fs::remove_file(TEST_DB_PATH).unwrap();
        }
        if Path::new(TEST_CACHE_DIR).exists() {
            fs::remove_dir_all(TEST_CACHE_DIR).unwrap();
        }
    }

    #[test]
    fn test_new() {
        setup();

        let context = Context::new(TEST_DB_PATH, TEST_CACHE_DIR);

        assert_eq!(context.db_path, TEST_DB_PATH);
        assert_eq!(context.cache_dir, TEST_CACHE_DIR);

        cleanup();
    }

    #[test]
    fn test_get_app_info() {
        setup();

        // Test case where app info is retrieved successfully from server.
        let mock_response: serde_json::Value = json!(
            {
                "msg": "success",
                "data": {
                    "appId": "fbbca092cdce4694a3d43f1ba002b6f1",
                    "name": "jzhomeland",
                    "force": false,
                    "useAppStore": false,
                    "appUri": "http://www.nikoeureka33.gr",
                    "version": 1,
                    "os": "Android",
                    "addTime": "2023-02-03T09:27:45.000Z",
                    "entrySiteID": "750e198198974a218a3e4c47cdb89d56",
                    "delFlag": 0,
                    "metaInfo": {},
                    "entry": {
                        "id": "0c0ed7fd985049d5a3dd9ae827e06e65",
                        "appId": "fbbca092cdce4694a3d43f1ba002b6f1",
                        "ttf": "",
                        "publish": null,
                        "version": "1.0.0",
                        "os": "Android",
                        "agent": "RN",
                        "addTime": "2023-02-03T09:27:45.000Z",
                        "script": [
                            {
                                "id": 9,
                                "moduleId": "0c0ed7fd985049d5a3dd9ae827e06e65",
                                "src": "http://10.20.0.18:3000/vendor_runtime_base.a5efc7ad05f6e4c152b1173fbf45f5ba.bundle.js",
                                "async": false,
                                "addTime": "2023-02-03T09:27:45.000Z"
                            },
                            {
                                "id": 10,
                                "moduleId": "0c0ed7fd985049d5a3dd9ae827e06e65",
                                "src": "http://10.20.0.18:3000/main.0cf488c4cf7f6668c126.bundle.js",
                                "async": false,
                                "addTime": "2023-02-03T09:27:45.000Z"
                            }
                        ]
                    }
                },
                "code": 0
            }
        );
        let url = format!("/appinfo/fbbca092cdce4694a3d43f1ba002b6f1");
        println!("mock_url: {:}", url);
        // Start a lightweight mock server.
        let mock_server = MockServer::start();

        // Create a mock on the server.
        let _mock = mock_server.mock(|when, then| {
            when.method(GET).path(url);
            then.status(200)
                .header("content-type", "application/json; charset=utf-8")
                .body(mock_response.to_string());
        });

        let context = Context::new(TEST_DB_PATH, TEST_CACHE_DIR);
        let server = &mock_server.base_url();
        let app_id = "fbbca092cdce4694a3d43f1ba002b6f1";

        let app_info = context.get_app_info(server, app_id).unwrap();
        println!("app_info: {:?}", app_info);
        assert!(app_info.is_some());

        cleanup();
    }

    #[test]
    fn test_get_resource() -> Result<(), Box<dyn std::error::Error>> {
        setup();
        // Set up a mock server that returns a JavaScript file
        let js_content = "console.log('Hello, world!');";
        // Start a lightweight mock server.
        let mock_server = MockServer::start();

        // Create a mock on the server.
        let _mock = mock_server.mock(|when, then| {
            when.method(GET).path("/test.js");
            then.status(200)
                .header("content-type", "application/javascript")
                .body(js_content);
        });
        // Initialize the resource manager
        let rm = Context::new(TEST_DB_PATH, TEST_CACHE_DIR);

        // Test getting a resource that doesn't exist
        let url = "https://example.com/nonexistent_resource";
        let disable_cache = false;
        let result = rm.get_resource(url, disable_cache);
        assert!(matches!(result, ApplicationError));

        // Test getting a resource that exists and is cached
        let url = &format!("{}/{}", mock_server.base_url(), "test.js");
        let disable_cache = false;
        let file_name = download(url, TEST_CACHE_DIR)?;
        let file_path = Path::new(TEST_CACHE_DIR).join(file_name);
        let hash_code = md5_file(&file_path)?;
        let file_path = file_path.to_string_lossy().to_string();
        let cache_ctrl = "";
        let conn: Connection = Connection::open(TEST_DB_PATH)?;
        conn.execute(
            "INSERT INTO Resource (url, path, hash_code, cache_ctrl) VALUES (?1, ?2, ?3, ?4)",
            params![url, file_path, hash_code, cache_ctrl],
        )?;
        let result = rm.get_resource(url, disable_cache)?;
        assert_eq!(result, Some(file_path));

        // Test getting a resource that exists but is not cached
        let url = &format!("{}/{}", mock_server.base_url(), "test.js");
        let disable_cache = false;
        let result = rm.get_resource(url, disable_cache)?;
        assert!(result.is_some());
        let content = fs::read_to_string(result.unwrap())?;
        assert_eq!(content, js_content);

        cleanup();
        Ok(())
    }
}
