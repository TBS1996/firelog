#![allow(non_snake_case)]

use dioxus::prelude::*;
use futures::executor::block_on;
use js_sys::Date;
use js_sys::Promise;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::Level;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::console;
use web_sys::{window, Storage};

type UnixTime = Duration;

type TaskID = Uuid;

const DEFAULT_SLOPE: f32 = std::f32::consts::E + 1.;

#[derive(Default, Clone)]
struct AuthUser {
    // #[allow(dead_code)]
    //  email: String,
    uid: String,
    //   #[allow(dead_code)]
    //   token: String,
}

impl AuthUser {
    fn from_jsvalue(val: wasm_bindgen::JsValue) -> Self {
        use gloo_utils::format::JsValueSerdeExt;

        let wtf: serde_json::Value = JsValueSerdeExt::into_serde(&val).unwrap();
        let obj = wtf.as_object().unwrap();

        let uid = obj.get("uid").unwrap().as_str().unwrap().to_owned();
        /*
        let token = obj
            .get("stsTokenManager")
            .unwrap()
            .as_object()
            .unwrap()
            .get("accessToken")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let email = obj.get("providerData").unwrap().as_array().unwrap()[0]
            .as_object()
            .unwrap()
            .get("email")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        */

        Self { uid }
    }
}

#[derive(Default, Clone)]
enum AuthStatus {
    Auth(AuthUser),
    #[default]
    Nope,
}

impl AuthStatus {
    fn user(&self) -> Option<AuthUser> {
        if let Self::Auth(user) = &self {
            Some(user.clone())
        } else {
            None
        }
    }

    fn is_authed(&self) -> bool {
        match self {
            Self::Nope => false,
            Self::Auth(_) => true,
        }
    }
}

#[derive(Clone, Default)]
struct StateInner {
    auth_status: Signal<AuthStatus>,
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
            auth_status: Signal::new(AuthStatus::Nope),
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

impl State {
    fn load() -> Self {
        log("lets load");
        let s = Self {
            inner: Arc::new(Mutex::new(StateInner::load())),
        };
        log("ok loaded lol");
        s
    }

    fn auth_user(&self) -> Option<AuthUser> {
        let state = use_context::<State>();
        let x = (*state.inner.lock().unwrap().auth_status.read()).clone();
        x.user()
    }

    fn refresh() {
        let state = use_context::<State>();
        let mut tasks = state.inner.lock().unwrap().tasks.clone();
        let mut value_stuff = state.inner.lock().unwrap().value_stuff.clone();
        tasks.set(task_props());
        value_stuff.set(tot_value_since());
    }
}

#[wasm_bindgen(module = "/assets/firestore.js")]
extern "C" {
    fn upsertFirestoreTask(user_id: &JsValue, id: &JsValue, task: &JsValue) -> Promise;
    fn loadAllTasks(user_id: &JsValue) -> Promise;
    fn addFirestoreTaskLog(user_id: &JsValue, task_id: &JsValue, log_id: &JsValue) -> Promise;
    fn loadLogsForTask(user_id: &JsValue, task_id: &JsValue) -> Promise;
    fn signInWithGoogle() -> Promise;
    fn signOutUser() -> Promise;
    fn xonAuthStateChanged(callback: &JsValue);
    fn getCurrentUser() -> JsValue;
}

async fn load_logs_for_task(user_id: String, task_id: Uuid) -> JsFuture {
    let task_id_str = task_id.to_string();
    let user_id = JsValue::from_str(&user_id);
    let task_id = JsValue::from_str(&task_id_str);

    let promise = loadLogsForTask(&user_id, &task_id);
    wasm_bindgen_futures::JsFuture::from(promise)
}

fn add_task_log_to_firestore(user_id: String, task_id: Uuid, timestamp: UnixTime) -> JsFuture {
    let task_id_str = task_id.to_string();
    let log_id_str = timestamp.as_secs().to_string();

    let user_id = JsValue::from_str(&user_id);
    let task_id = JsValue::from_str(&task_id_str);
    let log_id = JsValue::from_str(&log_id_str);

    let promise = addFirestoreTaskLog(&user_id, &task_id, &log_id);

    wasm_bindgen_futures::JsFuture::from(promise)
}

fn send_task_to_firestore(user_id: String, task: &Task) -> JsFuture {
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

#[derive(Default)]
struct SyncResult {
    send_up: Vec<Task>,
    download: HashMap<Uuid, MetaData>,
}

#[derive(Default, Debug)]
struct Syncer {
    pairs: Vec<(Task, MetaData)>,
    new_from_server: HashMap<Uuid, MetaData>,
    new_offline: Vec<Task>,
}

impl Syncer {
    fn new(mut online: HashMap<Uuid, MetaData>, offline: Tasks) -> Self {
        let mut selv = Self::default();
        for (_, off_task) in offline.0 {
            match online.remove(&off_task.id) {
                Some(ontask) => {
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

    fn sync(self) -> SyncResult {
        let mut res = SyncResult::default();
        for (off, on) in self.pairs {
            if off.metadata.updated > on.updated {
                res.send_up.push(off);
            } else if off.metadata.updated < on.updated {
                res.download.insert(off.id, on);
            }
        }

        for task in self.new_from_server {
            res.download.insert(task.0, task.1);
        }

        for task in self.new_offline {
            res.send_up.push(task);
        }

        res
    }
}

fn sync_tasks() {
    let state = use_context::<State>();

    let x = (*state.inner.lock().unwrap().auth_status.read()).clone();

    let mut tasks = state.inner.lock().unwrap().tasks.clone();
    let mut value_stuff = state.inner.lock().unwrap().value_stuff.clone();

    let Some(user) = x.user() else {
        tasks.set(task_props());
        value_stuff.set(tot_value_since());
        return;
    };

    let task_future =
        wasm_bindgen_futures::JsFuture::from(loadAllTasks(&JsValue::from_str(&user.uid)));
    let offline_tasks = Tasks::load_offline();

    wasm_bindgen_futures::spawn_local(async move {
        let online_tasks = MetaData::from_jsvalue(task_future.await.unwrap());

        let res = Syncer::new(online_tasks, offline_tasks).sync();

        for task in res.send_up {
            let future = send_task_to_firestore(user.uid.clone(), &task);
            future.await.unwrap();
        }

        for (id, task) in res.download {
            let metadata = MetaData {
                name: task.name,
                value: task.value,
                length: task.length,
                created: task.created,
                updated: task.updated,
                deleted: task.deleted,
            };

            metadata.save_offline(id).await;
        }

        log("syncing logs");
        let all_tasks = fetch_tasks().await;

        // load all firestore logs and merge with offline ones
        for id in all_tasks.into_keys() {
            let offline_logs = TaskLog::load_logs(id).await;
            let online_logs = {
                let val = load_logs_for_task(user.uid.clone(), id)
                    .await
                    .await
                    .unwrap();

                TaskLog::from_jsvalue(val)
            };

            let res = TaskLog::sync(online_logs, offline_logs);

            res.save.save_offline(id).await;

            for log in res.send_up {
                add_task_log_to_firestore(user.uid.clone(), id, log)
                    .await
                    .unwrap();
            }
        }

        tasks.set(task_props());
        value_stuff.set(tot_value_since());
    });
}

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home {},
    #[route("/new")]
    New {},
    #[route("/about")]
    About {},
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
    let mut tasks = Tasks::load_offline();
    tasks.prune_deleted();
    let tasks = tasks.to_vec_sorted();

    for task in tasks {
        value += task.value_since(dur);
    }

    value
}

#[component]
fn About() -> Element {
    rsx! {
        Link {to: Route::Home{}, "back"}

        p {
            "firelog, it's yet another task manager! but with a twist"
        }

        p {"basically, each task/habit has a value, you should use your own currency"}
        p {"recurring tasks get more important the longer since you did it (e.g. cleaning your room)"}
        p {"the 'value' basically means, if you were unable to do this task at a given moment, how much money would you pay to have it done?"}
        p {"since you also write in how long it takes to do the task, the value divided by the length (in hours) gives you the 'hourly wage' of each task"}
        p {"this means it'll ideally tell you which task has the best ROI at any given moment"}


    }
}

#[component]
fn Edit(id: Uuid) -> Element {
    let mut name = Signal::new(String::new());
    let mut length = Signal::new(String::new());
    let mut interval = Signal::new(String::new());
    let mut factor = Signal::new(String::new());

    let task = Tasks::load_offline().get_task(id).unwrap();
    let xinterval = task.interval();
    let xfactor = task.factor();

    let mut oldtask = task.clone();
    log(&oldtask);
    let navigator = use_navigator();

    rsx! {


        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",


            div {
                background_color: "lightblue",
                padding: "20px",

            button {
                onclick: move |_| {
                    navigator.replace(Route::Home{});
                },
                "go back"
            }
            button {
                onclick: move |_| {
                    Tasks::load_offline().delete_task(id);
                    State::refresh();
                    navigator.replace(Route::Home{});
                },
                "delete task"
            }




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
                    oldtask.metadata.name = newtask.metadata.name;
                    oldtask.metadata.length = newtask.metadata.length;
                    oldtask.metadata.updated = current_time();

                    let mut all_tasks = Tasks::load_offline();
                    all_tasks.insert(oldtask.clone());
                    all_tasks.save_offline();
                } else {

                log("fail!");
                };

                navigator.replace(Route::Home{});
                State::refresh();

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
                        value: task.metadata.name,
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
                        value: dur_to_mins(task.metadata.length),
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
    let auth = (*state.inner.lock().unwrap().auth_status.clone().read()).clone();

    let navigator = navigator();

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
                    if let Some(user) = auth.user() {
                        let future = send_task_to_firestore(user.uid.clone(),&task);
                        wasm_bindgen_futures::spawn_local(async {
                            future.await.unwrap();
                        });
                    }

                    let mut the_tasks = Tasks::load_offline();
                    the_tasks.insert(task);
                    the_tasks.save_offline();
                }

                navigator.replace(Route::Home{});
                State::refresh();

            },
            div {
                class: "input-group",
                display: "flex",
                flex_direction: "column",



                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    { tooltip("name", "name of task") }
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
                    { tooltip("length", "how many minutes does it take to finish this task?") }
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
                    { tooltip("interval", "approximately how often do you want to do this task, in days?") }
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
                    { tooltip("value", "after {interval} days have passed, how much would you pay to have this task done if you couldn't have it done yourself?") }
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

fn tooltip(main_text: &str, tooltip: &str) -> Element {
    rsx! {
        div {
        class: "tooltip-container",
        "{main_text}",
            div {
                class: "tooltip-text",
                "{tooltip}"
            }
        }
    }
}

#[component]
fn Home() -> Element {
    let state = use_context::<State>();

    let mut tasks = state.inner.lock().unwrap().tasks.clone();
    let mut value_stuff = state.inner.lock().unwrap().value_stuff.clone();
    let mut auth = state.inner.lock().unwrap().auth_status.clone();

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




                    if (*auth.read()).is_authed(){
                        button {
                            onclick: move |_| {
                                sync_tasks();
                                tasks.set(task_props());
                                value_stuff.set(tot_value_since());
                            },
                            "sync"
                        }

                    } else {
                        button {
                            onclick: move |_| {
                                let promise = signInWithGoogle();
                                let future = wasm_bindgen_futures::JsFuture::from(promise);
                                wasm_bindgen_futures::spawn_local(async move{
                                    let val = future.await.unwrap();
                                    let user = AuthUser::from_jsvalue(val);
                                    *auth.write() = AuthStatus::Auth(user);
                                });


                            },
                            "sign in",
                        }
                    }

                    button {
                        onclick: move |_| {
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
                                margin_right: "5px",
                                onclick: move |_| {
                                    log_to_console(&task.name);
                                    Tasks::load_offline().do_task(task.id);
                                    State::refresh();
                                },
                                "✅"
                            }
                            span {

                                margin_right: "5px",
                                "{task.priority}" }


                            Link { to: Route::Edit {id: task.id}, "{task.name}" }
                        }
                    }
                }
            }
                Link { to: Route::About {}, "about" }
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
        self.0.retain(|_, task| !task.metadata.deleted);
    }

    fn save_offline(&self) {
        log("starting save tasks");

        let mut metamap: HashMap<TaskID, MetaData> = HashMap::default();

        for (key, task) in &self.0 {
            metamap.insert(*key, task.metadata.clone());
        }

        Self::save_metadatas(metamap);
    }

    fn save_metadatas(metamap: HashMap<TaskID, MetaData>) {
        let s = serde_json::to_string(&metamap).unwrap();

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
        task.metadata.deleted = true;
        task.metadata.updated = current_time();
        self.insert(task);
        self.save_offline();
    }

    fn do_task(&mut self, id: Uuid) {
        let mut task = self.get_task(id).unwrap();
        task.do_task();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaData {
    name: String,
    value: ValueEq,
    length: Duration,
    created: UnixTime,
    updated: UnixTime,
    deleted: bool,
}

impl MetaData {
    fn new(name: impl Into<String>, equation: LogPriority, length: Duration) -> Self {
        let time = current_time();
        Self {
            name: name.into(),
            created: time,
            updated: time,
            value: ValueEq::Log(equation),
            deleted: false,
            length,
        }
    }

    async fn save_offline(&self, id: TaskID) {
        log("starting save tasks");

        let mut metamap = fetch_metadata().await;
        metamap.insert(id, self.clone());
        Tasks::save_metadatas(metamap);
    }

    fn from_jsvalue(val: wasm_bindgen::JsValue) -> HashMap<Uuid, Self> {
        let x: serde_json::Value = serde_wasm_bindgen::from_value(val).unwrap();
        log(("firetask: ", &x));
        let x = x.as_array().unwrap();

        let mut online_tasks = HashMap::default();

        for y in x {
            let task = y.get("task").unwrap().as_str().unwrap();
            let id = y.get("id").unwrap().as_str().unwrap();
            let task: MetaData = serde_json::from_str(&task).unwrap();
            let id: Uuid = serde_json::from_str(&id).unwrap();
            online_tasks.insert(id, task);
        }

        online_tasks
    }
}

#[derive(Default)]
struct LogSyncRes {
    send_up: Vec<Duration>,
    save: TaskLog,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TaskLog(Vec<UnixTime>);

impl TaskLog {
    fn new(&mut self, time: UnixTime) {
        if !self.0.contains(&time) {
            self.0.push(time);
        }
    }

    fn last_completed(&self) -> Option<UnixTime> {
        self.0.last().copied()
    }

    fn sync(from_online: Self, from_offline: Self) -> LogSyncRes {
        let mut res = LogSyncRes::default();
        let mut send_up = vec![];
        let mut set: HashSet<UnixTime> = HashSet::default();

        for unix in from_offline.0 {
            if !from_online.0.contains(&unix) {
                send_up.push(unix);
            }

            set.insert(unix);
        }

        for unix in from_online.0 {
            set.insert(unix);
        }

        send_up.sort();
        res.send_up = send_up;

        let x: Vec<Duration> = set.into_iter().collect();
        res.save = Self(x);

        res
    }

    fn from_jsvalue(val: JsValue) -> Self {
        let mut logs = vec![];
        let val: serde_json::Value = serde_wasm_bindgen::from_value(val).unwrap();
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
        Self(logs)
    }

    fn merge(&mut self, other: Self) {
        let mut set: HashSet<UnixTime> = HashSet::default();

        for log in &self.0 {
            set.insert(*log);
        }

        for log in other.0 {
            set.insert(log);
        }

        let mut vec: Vec<UnixTime> = set.into_iter().collect();
        vec.sort();

        *self = Self(vec);
    }

    async fn load_logs(task: TaskID) -> Self {
        fetch_logs().await.get(&task).cloned().unwrap_or_default()
    }

    async fn save_offline(&self, id: TaskID) {
        let mut all_logs = fetch_logs().await;
        let mut current = all_logs.get(&id).cloned().unwrap_or_default();
        current.merge(self.clone());
        all_logs.insert(id, current);

        log("starting save logs");
        let s = serde_json::to_string(&all_logs).unwrap();

        let storage: Storage = window()
            .expect("no global `window` exists")
            .local_storage()
            .expect("no local storage")
            .expect("local storage unavailable");

        storage
            .set_item("logs", &s)
            .expect("Unable to set item in local storage");
        log_to_console("Stored logs in local storage");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: TaskID,
    log: TaskLog,
    metadata: MetaData,
}

impl Task {
    fn new(name: impl Into<String>, equation: LogPriority, length: Duration) -> Self {
        Self {
            id: Uuid::new_v4(),
            metadata: MetaData::new(name, equation, length),
            log: TaskLog::default(),
        }
    }

    fn time_since_last_completion(&self) -> Duration {
        let created = self.metadata.created;
        current_time() - self.log.last_completed().unwrap_or(created)
    }

    fn set_interval(&mut self, interval: Duration) {
        if let ValueEq::Log(ref mut l) = &mut self.metadata.value {
            l.interval = interval;
            return;
        }

        panic!();
    }

    fn set_factor(&mut self, factor: f32) {
        if let ValueEq::Log(ref mut l) = &mut self.metadata.value {
            l.factor = factor;
            return;
        }

        panic!();
    }

    fn factor(&self) -> f32 {
        if let ValueEq::Log(l) = &self.metadata.value {
            return l.factor;
        }

        panic!();
    }

    fn interval(&self) -> Duration {
        if let ValueEq::Log(l) = &self.metadata.value {
            return l.interval;
        }

        panic!();
    }

    fn do_task(&mut self) {
        let current = current_time();

        self.log.new(current);
        block_on(self.log.save_offline(self.id));

        let state = use_context::<State>();
        if let Some(user) = state.auth_user() {
            let future = add_task_log_to_firestore(user.uid, self.id, current);
            wasm_bindgen_futures::spawn_local(async {
                match future.await {
                    Ok(_) => web_sys::console::log_1(&JsValue::from_str("Log added successfully")),
                    Err(e) => web_sys::console::log_1(&e),
                }
            });
        }
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

        let val = self.metadata.value.value(t);
        let hour_length = self.metadata.length.as_secs_f32() / 3600.;
        val / hour_length
    }

    fn value_since(&self, dur: Duration) -> f32 {
        let mut value_accrued = 0.;
        let mut prev_done = self.metadata.created;
        let current_time = current_time();
        for completed_time in &self.log.0 {
            let time_elapsed = *completed_time - prev_done;

            if current_time - *completed_time < dur {
                let value = self.metadata.value.value(time_elapsed);
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

async fn fetch_logs() -> HashMap<Uuid, TaskLog> {
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

    log(&logs_str);

    log_to_console("Completed localStorage call");

    match logs_str {
        Some(str) => {
            log_to_console(&format!("String from localStorage: {}", str));
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

async fn fetch_tasks() -> HashMap<Uuid, Task> {
    log("fetching tasks");
    let metadata = fetch_metadata().await;
    let logs = fetch_logs().await;

    log(("metadata: ", &metadata));
    log(("logs: ", &logs));

    let mut tasks = HashMap::default();

    for (key, metadata) in metadata {
        let log = logs.get(&key).cloned().unwrap_or_default();
        let task = Task {
            id: key,
            log,
            metadata,
        };
        tasks.insert(key, task);
    }

    tasks
}

async fn fetch_metadata() -> HashMap<Uuid, MetaData> {
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
            name: task.metadata.name.clone(),
            priority: format!("{:.2}", task.priority()),
            id: task.id,
        }
    }
}
