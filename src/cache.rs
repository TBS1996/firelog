use crate::task::{Task, TaskLog};
use crate::{log, log_to_console, MetaData};
use std::collections::HashMap;
use uuid::Uuid;
use web_sys::{window, Storage};

pub fn save_uid(uid: &str) {
    save("uid", uid);
}

pub async fn load_uid() -> Option<String> {
    load("uid").await
}

pub fn save_logs(logs: HashMap<Uuid, TaskLog>) {
    let s = serde_json::to_string(&logs).unwrap();
    save("logs", &s);
}

pub async fn fetch_logs() -> HashMap<Uuid, TaskLog> {
    log_to_console("Starting fetch_logs");
    let logs_str = load("logs").await;
    log_to_console("Completed localStorage call");

    match logs_str {
        Some(str) => serde_json::from_str(&str).unwrap_or_else(|e| {
            log_to_console(&format!("Deserialization error: {:?}", e));
            HashMap::default()
        }),
        None => {
            log_to_console("No logs found in localStorage");
            HashMap::default()
        }
    }
}

pub async fn fetch_tasks() -> HashMap<Uuid, Task> {
    log("fetching tasks");
    let metadata = fetch_metadata().await;
    let logs = fetch_logs().await;

    let mut tasks = HashMap::default();

    for (key, metadata) in metadata {
        let log = logs.get(&key).cloned().unwrap_or_default();
        let task = Task {
            id: key,
            log,
            metadata,
        };
        log_to_console(("fetching task: ", &task.metadata.name));
        tasks.insert(key, task);
    }
    tasks
}

pub async fn fetch_metadata() -> HashMap<Uuid, MetaData> {
    log_to_console("Starting fetch_tasks");
    let tasks_str = load("tasks").await;
    log_to_console("Completed localStorage call");

    match tasks_str {
        Some(str) => serde_json::from_str(&str).unwrap_or_else(|e| {
            log_to_console(&format!("Deserialization error: {:?}", e));
            HashMap::default()
        }),
        None => {
            log_to_console("No tasks found in localStorage");
            HashMap::default()
        }
    }
}

fn storage() -> Storage {
    window()
        .expect("no global `window` exists")
        .local_storage()
        .expect("no local storage")
        .expect("local storage unavailable")
}

fn save(key: &str, val: &str) {
    storage()
        .set_item(key, val)
        .expect("Unable to set item in local storage");
}

async fn load(key: &str) -> Option<String> {
    storage().get_item(key).unwrap_or_else(|_| {
        log_to_console("Error retrieving item from local storage");
        None
    })
}
