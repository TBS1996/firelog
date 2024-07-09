#![allow(non_snake_case)]

use dioxus::prelude::*;
use futures::executor::block_on;
use js_sys::Date;
use js_sys::Promise;
use std::collections::HashMap;
use std::time::Duration;
use tracing::Level;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::console;

type UnixTime = Duration;

const DEFAULT_SLOPE: f32 = std::f32::consts::E + 1.;

#[derive(Clone, Default)]
struct StateInner {
    name: Signal<String>,
    length: Signal<String>,
    interval: Signal<String>,
    factor: Signal<String>,
    tasks: Signal<Vec<TaskProp>>,
    value_stuff: Signal<f32>,
}

impl StateInner {
    fn load() -> Self {
        Self {
            name: Signal::new(String::new()),
            length: Signal::new(String::new()),
            interval: Signal::new(String::new()),
            factor: Signal::new(String::new()),
            tasks: Signal::new(task_props()),
            value_stuff: Signal::new(tot_value_since()),
        }
    }
}

#[derive(Clone)]
struct State {
    inner: Arc<Mutex<StateInner>>,
}

use std::sync::{Arc, Mutex};

impl State {
    fn load() -> Self {
        log("lets load");
        let s = Self {
            inner: Arc::new(Mutex::new(StateInner::load())),
        };
        log("ok loaded lol");
        s
    }
}

#[wasm_bindgen(module = "/assets/firestore.js")]
extern "C" {
    fn upsertFirestoreTask(id: &JsValue, task: &JsValue) -> Promise;
    fn loadAllTasks() -> Promise;
    fn addFirestoreTaskLog(task_id: &JsValue, log_id: &JsValue) -> Promise;
    fn loadLogsForTask(task_id: &JsValue) -> Promise;
}

async fn load_logs_for_task(task_id: Uuid) -> JsFuture {
    let task_id_str = task_id.to_string();
    let task_id = JsValue::from_str(&task_id_str);

    let promise = loadLogsForTask(&task_id);
    wasm_bindgen_futures::JsFuture::from(promise)
}

fn add_task_log_to_firestore(task_id: Uuid, timestamp: UnixTime) -> JsFuture {
    let task_id_str = task_id.to_string();
    let log_id_str = timestamp.as_secs().to_string();

    let task_id = JsValue::from_str(&task_id_str);
    let log_id = JsValue::from_str(&log_id_str);

    let promise = addFirestoreTaskLog(&task_id, &log_id);

    wasm_bindgen_futures::JsFuture::from(promise)
}

#[derive(Default, Debug)]
struct Syncer {
    pairs: Vec<(Task, FireTask)>,
    new_from_server: Vec<FireTask>,
    new_offline: Vec<Task>,
}

impl Syncer {
    fn new(mut online: Vec<FireTask>, offline: Tasks) -> Self {
        let mut selv = Self::default();
        for (_, off_task) in offline.0 {
            let pos = online.iter().position(|ontask| ontask.id == off_task.id);
            match pos {
                Some(pos) => {
                    let ontask = online.remove(pos);
                    selv.pairs.push((off_task, ontask));
                }
                None => {
                    selv.new_offline.push(off_task);
                }
            };
        }

        selv.new_from_server = online;

        selv
    }

    fn sync(self) -> (Tasks, Vec<JsFuture>, Vec<FireTask>) {
        log("lets sync");
        log(&self);
        let mut offline_tasks = Tasks::default();
        let mut new_tasks = vec![];
        let mut futures = vec![];
        log("lol");
        for (off, on) in self.pairs {
            log("hey");
            if off.updated > on.updated {
                let future = send_task_to_firestore(&off);
                futures.push(future);
            } else if off.updated < on.updated {
                new_tasks.push(on);
            }
            offline_tasks.insert(off);
        }

        for task in self.new_from_server {
            log(("from server: ", &task));
            new_tasks.push(task);
            log("XD");
        }

        log("new offline");
        for task in self.new_offline {
            let future = send_task_to_firestore(&task);
            futures.push(future);
            offline_tasks.insert(task);
        }

        log("sync func done");
        (offline_tasks, futures, new_tasks)
    }
}

fn sync_tasks() {
    let task_future = wasm_bindgen_futures::JsFuture::from(loadAllTasks());
    let offline_tasks = Tasks::load_offline();

    let state = use_context::<State>();
    let mut tasks = state.inner.lock().unwrap().tasks.clone();
    let mut value_stuff = state.inner.lock().unwrap().value_stuff.clone();

    wasm_bindgen_futures::spawn_local(async move {
        let online_tasks = {
            let x = task_future.await.unwrap();
            let x: serde_json::Value = serde_wasm_bindgen::from_value(x).unwrap();
            let x = x.as_array().unwrap();

            let mut online_tasks = vec![];

            for y in x {
                let task = y.get("task").unwrap().as_str().unwrap();
                let task: FireTask = serde_json::from_str(&task).unwrap();
                online_tasks.push(task);
            }

            online_tasks
        };

        let (mut offtask, futures, newtasks) = Syncer::new(online_tasks, offline_tasks).sync();

        for future in futures {
            log("lets await");
            future.await.unwrap();
        }

        log("cool");
        for task in newtasks {
            let x = load_logs_for_task(task.id).await.await.unwrap();

            let mut logs = vec![];
            let val: serde_json::Value = serde_wasm_bindgen::from_value(x.clone()).unwrap();
            let mm = val.as_array().unwrap().clone();

            for x in mm {
                let ts: u64 = x
                    .as_object()
                    .unwrap()
                    .get("timestamp")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap();

                let ts = UnixTime::from_secs(ts);
                logs.push(ts);
            }

            logs.sort();

            let t = Task {
                name: task.name,
                value: task.value,
                length: task.length,
                created: task.created,
                updated: task.updated,
                deleted: task.deleted,
                id: task.id,
                log: logs,
            };

            offtask.insert(t);
        }

        offtask.save_offline();

        for (id, task) in &offtask.0 {
            for log in &task.log {
                add_task_log_to_firestore(*id, *log).await.unwrap();
            }
        }

        tasks.set(task_props());
        value_stuff.set(tot_value_since());
    });
}

// Create a Rust function to send data to Firestore
fn send_task_to_firestore(task: &Task) -> JsFuture {
    let (task, id) = FireTask::new(task);
    let taskstr = serde_json::to_string(&task).unwrap();

    let task = js_sys::Object::new();
    js_sys::Reflect::set(
        &task,
        &JsValue::from_str("task"),
        &JsValue::from_str(&taskstr),
    )
    .unwrap();

    let idstr = serde_json::to_string(&id).unwrap();

    let id = JsValue::from_str(&idstr);

    // Call the JavaScript function
    // let promise = upsertFirestoreTask(&task, &id);
    let promise = upsertFirestoreTask(&id, &task);
    let future = wasm_bindgen_futures::JsFuture::from(promise);
    future
}

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home {},
    #[route("/new")]
    New {},
    #[route("/edit/:id")]
    Edit { id: Uuid },
}

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    launch(App);
}

fn App() -> Element {
    use_context_provider(State::load);
    rsx! {
        Router::<Route> {}
    }
}

fn task_props() -> Vec<TaskProp> {
    let mut tasks = Tasks::load_offline();
    tasks.prune_deleted();

    let tasks = tasks.to_vec_sorted();

    let tasks: Vec<TaskProp> = tasks.iter().map(|task| TaskProp::from_task(task)).collect();
    tasks
}

fn tot_value_since() -> f32 {
    let dur = Duration::from_secs(86400);
    let mut value = 0.;
    let tasks = Tasks::load_offline().to_vec_sorted();

    for task in tasks {
        value += task.value_since(dur);
    }

    value
}

#[component]
fn Edit(id: Uuid) -> Element {
    let mut name = Signal::new(String::new());
    let mut length = Signal::new(String::new());
    let mut interval = Signal::new(String::new());
    let mut factor = Signal::new(String::new());

    let mut task = Tasks::load_offline().get_task(id).unwrap();
    let xinterval = task.interval();
    let xfactor = task.factor();

    let mut oldtask = task.clone();
    log(&oldtask);

    rsx! {


        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",


            div {
                background_color: "lightblue",
                padding: "20px",

            Link { to: Route::Home {}, "back" }




        form {
            display: "flex",
            flex_direction: "row",
            onsubmit: move |event| {
                let data = event.data().values();
                log("submitting!");
                let newtask = Task::from_form(data);

                if let Some(newtask) = newtask {
                log("success!");
                    oldtask.set_factor(newtask.factor());
                    oldtask.set_interval(newtask.interval());
                    oldtask.name = newtask.name;
                    oldtask.length = newtask.length;
                    oldtask.updated = current_time();

                    let mut all_tasks = Tasks::load_offline();
                    all_tasks.insert(oldtask.clone());
                    all_tasks.save_offline();
                } else {

                log("fail!");
                };

            },
            div {
                class: "input-group",
                display: "flex",
                flex_direction: "column",



                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    { "name" }
                    input {
                        r#type: "text",
                        value: task.name,
                        name: "name",
                        autocomplete: "off",
                        oninput: move |event| name.set(event.value()),
                    }
                }

                div {
                    flex_direction: "row",
                    display: "flex",
                    justify_content: "space-between",
                    { "length" }
                    input {
                        r#type: "number",
                        min: "1",
                        step: "any",
                        name: "length",
                        value: dur_to_mins(task.length),
                        autocomplete: "off",
                        oninput: move |event| length.set(event.value()),
                    }
                }

                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    { "interval" }
                    input {
                        r#type: "number",
                        min: "0.01",
                        step: "any",
                        name: "interval",
                        value: dur_to_days(xinterval),
                        autocomplete: "off",
                        oninput: move |event| interval.set(event.value()),
                    }
                }


                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    { "value" }
                    input {
                        r#type: "number",
                        name: "factor",
                        value: xfactor.to_string(),
                        autocomplete: "off",
                        oninput: move |event| factor.set(event.value()),
                    }
                }

                button {
                    r#type: "submit",
                    class: "confirm",
                    "Update task"
                }
           }



        }

            }
        }

    }
}

fn dur_to_days(dur: Duration) -> String {
    format!("{:.1}", dur.as_secs_f32() / 86400.)
}

fn dur_to_mins(dur: Duration) -> String {
    (dur.as_secs() / 60).to_string()
}

#[component]
fn New() -> Element {
    let state = use_context::<State>();

    let mut name = state.inner.lock().unwrap().name.clone();
    let mut length = state.inner.lock().unwrap().length.clone();
    let mut interval = state.inner.lock().unwrap().interval.clone();
    let mut factor = state.inner.lock().unwrap().factor.clone();

    log("neww");

    rsx! {

        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",


            div {
                background_color: "lightblue",
                padding: "20px",

            Link { to: Route::Home {}, "back" }


        form {
            display: "flex",
            flex_direction: "row",
            onsubmit: move |event| {
                name.set(String::new());
                length.set(String::new());
                interval.set(String::new());
                factor.set(String::new());

                let data = event.data().values();
                let task = Task::from_form(data);
                log_to_console(&task);

                if let Some(task) = task {
                    let future = send_task_to_firestore(&task);
                    wasm_bindgen_futures::spawn_local(async {
                        future.await.unwrap();
                    });

                    let mut the_tasks = Tasks::load_offline();
                    the_tasks.insert(task);
                    the_tasks.save_offline();
                }

            },
            div {
                class: "input-group",
                display: "flex",
                flex_direction: "column",



                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    { "name" }
                    input {
                        r#type: "text",
                        value: name(),
                        name: "name",
                        autocomplete: "off",
                        oninput: move |event| name.set(event.value()),
                    }
                }

                div {
                    flex_direction: "row",
                    display: "flex",
                    justify_content: "space-between",
                    { "length" }
                    input {
                        r#type: "number",
                        min: "1",
                        step: "any",
                        name: "length",
                        value: length(),
                        autocomplete: "off",
                        oninput: move |event| length.set(event.value()),
                    }
                }

                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    { "interval" }
                    input {
                        r#type: "number",
                        min: "0.01",
                        step: "any",
                        name: "interval",
                        value: interval(),
                        autocomplete: "off",
                        oninput: move |event| interval.set(event.value()),
                    }
                }


                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    { "value" }
                    input {
                        r#type: "number",
                        name: "factor",
                        value: factor(),
                        autocomplete: "off",
                        oninput: move |event| factor.set(event.value()),
                    }
                }

                button {
                    r#type: "submit",
                    class: "confirm",
                    "Create task"
                }
           }
        }
            }
        }

    }
}

#[component]
fn Home() -> Element {
    let state = use_context::<State>();
    log("111");
    //    let navigator = navigator();
    log("211");

    log("311");

    let mut tasks = state.inner.lock().unwrap().tasks.clone();
    let mut value_stuff = state.inner.lock().unwrap().value_stuff.clone();
    log("411");
    log("511");

    rsx! {
        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",
            flex_direction: "column",


            Link { to: Route::New {}, "New task!" }



            div {
                background_color: "lightblue",
                padding: "20px",

                div {
                    display: "flex",
                    flex_direction: "row",

                    button {
                        onclick: move |_| {
                            sync_tasks();
                            tasks.set(task_props());
                            value_stuff.set(tot_value_since());
                        },
                        "🔄"
                    }
                    div {"value last 24 hours: {value_stuff}"}

                }

                div {
                    display: "flex",
                    flex_direction: "column",
                    padding: "5px",

                    for task in tasks() {

                        div {
                            display: "flex",
                            flex_direction: "row",

                            button {
                                onclick: move |_| {
                                    Tasks::load_offline().delete_task(task.id);
                                    tasks.set(task_props());
                                    value_stuff.set(tot_value_since());
                                },
                                "❌"
                            }
                            button {
                                onclick: move |_| {
                                    log_to_console(&task.name);
                                    Tasks::load_offline().do_task(task.id);
                                    tasks.set(task_props());
                                    value_stuff.set(tot_value_since());
                                },
                                "✅"
                            }

                            Link { to: Route::Edit {id: task.id}, "{task.priority} {task.name}" }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogPriority {
    interval: UnixTime,
    factor: f32,
    slope: f32,
}

impl LogPriority {
    pub fn new(factor: f32, interval: Duration) -> Self {
        Self {
            interval,
            factor,
            slope: DEFAULT_SLOPE,
        }
    }

    fn common_factor(unity: f32, twonity: f32) -> f32 {
        let ratio = twonity / unity;
        (ratio - 2.) / unity
    }

    fn ab(unity: f32, twonity: f32) -> (f32, f32) {
        let common = Self::common_factor(unity, twonity);
        let a = common;
        let b = unity * common + 1.;
        (a, b)
    }

    fn value(&self, t: Duration) -> f32 {
        let t = t.as_secs_f32() / 86400.;
        let t1 = self.interval.as_secs_f32() / 86400.;
        let t2 = t1 * self.slope;

        let (a, b) = Self::ab(t1, t2);
        (a * t + 1.).log(b) * self.factor
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValueEq {
    Log(LogPriority),
    Const(f32),
}

impl ValueEq {
    fn value(&self, t: Duration) -> f32 {
        match self {
            Self::Log(log) => log.value(t),
            Self::Const(f) => *f,
        }
    }
}

use serde::{Deserialize, Serialize};

/// The way the firetask thing is stored in firesore.
/// so yeah, basically same but without the log and id. Cause log is stored separately
/// and id is the key in the store so no need for that to be here lol.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FireTask {
    name: String,
    value: ValueEq,
    length: Duration,
    created: UnixTime,
    updated: UnixTime,
    deleted: bool,
    id: Uuid,
}

impl FireTask {
    fn new(task: &Task) -> (Self, Uuid) {
        let selv = Self {
            name: task.name.clone(),
            value: task.value.clone(),
            length: task.length,
            created: task.created,
            updated: task.updated,
            deleted: task.deleted,
            id: task.id,
        };

        (selv, task.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Tasks(HashMap<Uuid, Task>);

impl Tasks {
    fn load_offline() -> Self {
        Self(block_on(fetch_tasks()))
    }

    fn to_vec_sorted(self) -> Vec<Task> {
        let mut vec = vec![];

        for (_, task) in self.0.into_iter() {
            vec.push(task);
        }

        vec.sort_by_key(|t| (t.priority() * 1000.) as u32);
        vec.reverse();

        vec
    }

    fn prune_deleted(&mut self) {
        self.0.retain(|_, task| !task.deleted);
    }

    fn save_offline(&self) {
        log("starting save tasks");
        let s = serde_json::to_string(&self.0).unwrap();
        let storage: Storage = window()
            .expect("no global `window` exists")
            .local_storage()
            .expect("no local storage")
            .expect("local storage unavailable");

        storage
            .set_item("tasks", &s)
            .expect("Unable to set item in local storage");
        log_to_console("Stored tasks in local storage");
    }

    fn get_task(&self, id: Uuid) -> Option<Task> {
        self.0.get(&id).cloned()
    }

    fn insert(&mut self, task: Task) {
        self.0.insert(task.id, task);
    }

    fn delete_task(&mut self, id: Uuid) {
        let mut task = self.get_task(id).unwrap();
        task.deleted = true;
        task.updated = current_time();
        self.insert(task);
        self.save_offline();
    }

    fn do_task(&mut self, id: Uuid) {
        let mut task = self.get_task(id).unwrap();
        task.do_task();
        self.save_offline();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    name: String,
    value: ValueEq,
    length: Duration,
    created: UnixTime,
    updated: UnixTime,
    log: Vec<UnixTime>,
    deleted: bool,
    id: Uuid,
}

impl Task {
    fn new(name: impl Into<String>, equation: LogPriority, length: Duration) -> Self {
        let time = current_time();
        Self {
            name: name.into(),
            created: time,
            updated: time,
            log: vec![],
            value: ValueEq::Log(equation),
            deleted: false,
            length,
            id: Uuid::new_v4(),
        }
    }

    fn set_interval(&mut self, interval: Duration) {
        if let ValueEq::Log(ref mut l) = &mut self.value {
            l.interval = interval;
            return;
        }

        panic!();
    }

    fn set_factor(&mut self, factor: f32) {
        if let ValueEq::Log(ref mut l) = &mut self.value {
            l.factor = factor;
            return;
        }

        panic!();
    }

    fn factor(&self) -> f32 {
        if let ValueEq::Log(l) = &self.value {
            return l.factor;
        }

        panic!();
    }

    fn interval(&self) -> Duration {
        if let ValueEq::Log(l) = &self.value {
            return l.interval;
        }

        panic!();
    }

    fn do_task(&mut self) {
        let current = current_time();
        self.log.push(current);

        let future = add_task_log_to_firestore(self.id, current);
        wasm_bindgen_futures::spawn_local(async {
            match future.await {
                Ok(_) => web_sys::console::log_1(&JsValue::from_str("Log added successfully")),
                Err(e) => web_sys::console::log_1(&e),
            }
        });
    }

    fn from_form(form: HashMap<String, FormValue>) -> Option<Self> {
        log("name");
        let name = form.get("name")?.as_value();
        log("factor");

        let factor: f32 = form.get("factor")?.as_value().parse().ok()?;
        log("interval");

        let interval = {
            let interval = form.get("interval")?.as_value();
            let days: f32 = interval.parse().ok()?;
            Duration::from_secs_f32(days * 86400.)
        };
        log("length");

        let length = {
            let length = form.get("length")?.as_value();
            let mins: f32 = length.parse().ok()?;
            Duration::from_secs_f32(mins * 60.)
        };
        log("logstuff");

        let logstuff = LogPriority::new(factor, interval);
        log("selv");

        Some(Self::new(name, logstuff, length))
    }

    /// Hourly wage
    fn priority(&self) -> f32 {
        let t = self.time_since_last_completion();

        let val = self.value.value(t);
        let hour_length = self.length.as_secs_f32() / 3600.;
        val / hour_length
    }

    fn time_since_last_completion(&self) -> Duration {
        current_time() - self.last_completed()
    }

    fn last_completed(&self) -> UnixTime {
        let last = match self.log.last() {
            Some(time) => *time,
            None => self.created,
        };
        last
    }

    fn value_since(&self, dur: Duration) -> f32 {
        let mut value_accrued = 0.;
        let mut prev_done = self.created;
        let current_time = current_time();
        for completed_time in &self.log {
            let time_elapsed = *completed_time - prev_done;

            if current_time - *completed_time < dur {
                let value = self.value.value(time_elapsed);
                value_accrued += value;
            }

            prev_done = *completed_time;
        }

        value_accrued
    }
}

pub fn current_time() -> UnixTime {
    let date = Date::new_0();
    let milliseconds_since_epoch = date.get_time() as u64;
    let seconds_since_epoch = milliseconds_since_epoch / 1000;
    UnixTime::from_secs(seconds_since_epoch)
}

pub fn log(message: impl std::fmt::Debug) {
    log_to_console(message);
}

pub fn log_to_console(message: impl std::fmt::Debug) {
    let message = format!("{:?}", message);
    console::log_1(&JsValue::from_str(&message));
}

use web_sys::{window, Storage};

async fn fetch_tasks() -> HashMap<Uuid, Task> {
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

    log(&tasks_str);

    log_to_console("Completed localStorage call");

    match tasks_str {
        Some(str) => {
            log_to_console(&format!("String from localStorage: {}", str));
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

#[derive(Props, PartialEq, Clone)]
struct TaskProp {
    name: String,
    priority: String,
    id: Uuid,
}

impl TaskProp {
    fn from_task(task: &Task) -> Self {
        Self {
            name: task.name.clone(),
            priority: format!("{:.2}", task.priority()),
            id: task.id,
        }
    }
}
