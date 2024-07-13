use js_sys::Promise;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::task::LogRecord;
use crate::Task;

#[wasm_bindgen(module = "/assets/firestore.js")]
extern "C" {
    fn upsertFirestoreTask(user_id: &JsValue, id: &JsValue, task: &JsValue) -> Promise;
    pub fn loadAllTasks(user_id: &JsValue) -> Promise;
    fn addFirestoreTaskLog(
        user_id: &JsValue,
        task_id: &JsValue,
        log_id: &JsValue,
        log_factor: &JsValue,
    ) -> Promise;
    fn loadLogsForTask(user_id: &JsValue, task_id: &JsValue) -> Promise;
    pub fn signInWithGoogle() -> Promise;
    fn signOutUser() -> Promise;
    fn xonAuthStateChanged(callback: &JsValue);
    fn getCurrentUser() -> JsValue;
}

pub async fn load_logs_for_task(user_id: String, task_id: Uuid) -> JsFuture {
    let task_id_str = task_id.to_string();
    let user_id = JsValue::from_str(&user_id);
    let task_id = JsValue::from_str(&task_id_str);

    let promise = loadLogsForTask(&user_id, &task_id);
    wasm_bindgen_futures::JsFuture::from(promise)
}

pub fn add_task_log_to_firestore(user_id: String, task_id: Uuid, log: LogRecord) -> JsFuture {
    let task_id_str = task_id.to_string();
    let log_id_str = log.time.as_secs().to_string();
    let unit_str = log.units.to_string();

    let user_id = JsValue::from_str(&user_id);
    let task_id = JsValue::from_str(&task_id_str);
    let log_id = JsValue::from_str(&log_id_str);
    let unit = JsValue::from_str(&unit_str);

    let promise = addFirestoreTaskLog(&user_id, &task_id, &log_id, &unit);

    wasm_bindgen_futures::JsFuture::from(promise)
}

pub fn send_task_to_firestore(user_id: String, task: &Task) -> JsFuture {
    let id = task.id;
    let task = task.metadata.clone();

    let taskstr = serde_json::to_string(&task).unwrap();

    let task = js_sys::Object::new();
    js_sys::Reflect::set(
        &task,
        &JsValue::from_str("task"),
        &JsValue::from_str(&taskstr),
    )
    .unwrap();

    let idstr = serde_json::to_string(&id).unwrap();

    let user_id = JsValue::from_str(&user_id);
    let id = JsValue::from_str(&idstr);

    let promise = upsertFirestoreTask(&user_id, &id, &task);
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    future
}
