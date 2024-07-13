use crate::task::{Task, TaskLog};
use crate::{log, log_to_console, MetaData};
use std::collections::HashMap;
use uuid::Uuid;
use web_sys::{window, Storage};

pub async fn fetch_logs() -> HashMap<Uuid, TaskLog> {
    log_to_console("Starting fetch_logs");

    let storage: Storage = window()
        .expect("no global `window` exists")
        .local_storage()
        .expect("no local storage")
        .expect("local storage unavailable");

    let logs_str = storage.get_item("logs").unwrap_or_else(|_| {
        log_to_console("Error retrieving item from local storage");
        None
    });

    //    log(&logs_str);

    log_to_console("Completed localStorage call");

    match logs_str {
        Some(str) => {
            // log_to_console(&format!("String from localStorage: {}", str));
            serde_json::from_str(&str).unwrap_or_else(|e| {
                log_to_console(&format!("Deserialization error: {:?}", e));
                HashMap::default()
            })
        }
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

    let storage: Storage = window()
        .expect("no global `window` exists")
        .local_storage()
        .expect("no local storage")
        .expect("local storage unavailable");

    let tasks_str = storage.get_item("tasks").unwrap_or_else(|_| {
        log_to_console("Error retrieving item from local storage");
        None
    });

    log_to_console("Completed localStorage call");

    match tasks_str {
        Some(str) => {
            // log_to_console(&format!("String from localStorage: {}", str));
            serde_json::from_str(&str).unwrap_or_else(|e| {
                log_to_console(&format!("Deserialization error: {:?}", e));
                HashMap::default()
            })
        }
        None => {
            log_to_console("No tasks found in localStorage");
            HashMap::default()
        }
    }
}

pub fn save(key: &str, val: &str) {
    let storage: Storage = window()
        .expect("no global `window` exists")
        .local_storage()
        .expect("no local storage")
        .expect("local storage unavailable");

    storage
        .set_item(key, val)
        .expect("Unable to set item in local storage");
    log_to_console("Stored tasks in local storage");
}

pub async fn load(key: &str) -> Option<String> {
    log_to_console("Starting fetch_tasks");

    let storage: Storage = window()
        .expect("no global `window` exists")
        .local_storage()
        .expect("no local storage")
        .expect("local storage unavailable");

    let tasks_str = storage.get_item(key).unwrap_or_else(|_| {
        log_to_console("Error retrieving item from local storage");
        None
    });

    log_to_console("Completed localStorage call");

    tasks_str
}
