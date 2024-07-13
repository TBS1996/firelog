#![allow(non_snake_case)]

use dioxus::prelude::*;
use futures::executor::block_on;
use js_sys::Date;
use js_sys::Promise;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contask {
    // How many units you're expected to do per day on avg
    daily_units: f32,
    //  when you do one unit at the average rate, how much is the value?
    factor: f32,

    created: UnixTime,

    unit_name: Option<String>,
}

impl Contask {
    fn new(daily_units: f32, factor: f32, unit_name: String) -> Self {
        let created = current_time();

        Self {
            daily_units,
            factor,
            created,
            unit_name: Some(unit_name),
        }
    }

    fn value(&self, logs: &TaskLog, current: UnixTime) -> f32 {
        self.factor * self.ratio(logs, current)
    }

    fn ratio(&self, logs: &TaskLog, current: UnixTime) -> f32 {
        let avg = self.daily_average(logs, current);
        log(("avg: ", avg));
        self.daily_units / avg
    }

    fn daily_average(&self, logs: &TaskLog, current: UnixTime) -> f32 {
        let days_elapsed = (current - self.created).as_secs_f32() / 86400.;
        let total_units: f32 = logs.0.iter().map(|log| log.units).sum();

        // We add the daily_units so that when you create the task for the first time the avg isnt
        // 0 and thus the value infinite.
        (total_units + self.daily_units) / (days_elapsed + 1.0)
    }
}

#[derive(Default, Clone)]
struct AuthUser {
    uid: String,
}

impl AuthUser {
    fn from_jsvalue(val: wasm_bindgen::JsValue) -> Self {
        use gloo_utils::format::JsValueSerdeExt;

        let wtf: serde_json::Value = JsValueSerdeExt::into_serde(&val).unwrap();
        let obj = wtf.as_object().unwrap();

        let uid = obj.get("uid").unwrap().as_str().unwrap().to_owned();

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
    tasktype: Signal<String>,
    tasks: Signal<Vec<TaskProp>>,
    value_stuff: Signal<f32>,
    is_syncing: Signal<bool>,
}

impl StateInner {
    fn load() -> Self {
        Self {
            auth_status: Signal::new(AuthStatus::Nope),
            tasktype: Signal::new(String::from("disc")),
            tasks: Signal::new(task_props()),
            value_stuff: Signal::new(tot_value_since()),
            is_syncing: Signal::new(false),
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
    fn addFirestoreTaskLog(
        user_id: &JsValue,
        task_id: &JsValue,
        log_id: &JsValue,
        log_factor: &JsValue,
    ) -> Promise;
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

fn add_task_log_to_firestore(user_id: String, task_id: Uuid, log: LogRecord) -> JsFuture {
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

fn sync_tasks(mut is_syncing: Signal<bool>) {
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
        is_syncing.set(true);
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

        is_syncing.set(false);
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
    #[route("/units/:id")]
    Units { id: Uuid },
    #[route("/about")]
    About {},
    #[route("/edit/:id")]
    Edit { id: Uuid },
    #[route("/editcont/:id")]
    Editcont { id: Uuid },
}

#[component]
fn Units(id: Uuid) -> Element {
    let mut task = Tasks::load_offline().get_task(id).unwrap();
    let unit_name = task.unit_name();

    let mut input = Signal::new(String::new());

    let navigator = navigator();

    rsx! {

        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",


            div {
                padding: "20px",

            Link { to: Route::Home {}, "back" }



        form {
            display: "flex",
            flex_direction: "row",
            onsubmit: move |event| {

                let data = event.data().values();
                let units: f32 = data.get("input").unwrap().as_value().to_string().parse().unwrap();
                task.do_task(units);
                navigator.replace(Route::Home {});
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
                    "{unit_name}"
                    input {
                        r#type: "text",
                        value: input(),
                        name: "input",
                        autocomplete: "off",
                        oninput: move |event| input.set(event.value()),
                    }
                }

                button {
                    r#type: "submit",
                    class: "confirm",
                    "submit"
                }
           }
        }
            }
        }

    }
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
    let time = current_time() - Duration::from_secs(86400);
    let mut value = 0.;
    let mut tasks = Tasks::load_offline();
    tasks.prune_deleted();
    let tasks = tasks.to_vec_sorted();

    for task in tasks {
        value += task.value_since(time);
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
fn Editcont(id: Uuid) -> Element {
    let task = Tasks::load_offline().get_task(id).unwrap();
    let thetask = Tasks::load_offline().get_task(id).unwrap();
    let ratio = task.ratio();
    let value = task
        .metadata
        .value
        .value(&task.log, task.metadata.created, current_time());

    let navigator = use_navigator();

    let logstr: Vec<String> = task
        .log
        .time_since(current_time())
        .into_iter()
        .map(|dur| dur_format(dur))
        .collect();
    let logstr = format!("{:?}", logstr);
    let mut logstr = logstr.replace("\"", "");
    logstr.pop();
    logstr.remove(0);

    let closure = move |newtask: Option<Task>| {
        let mut oldtask = task.clone();
        let newtask = newtask.unwrap();

        log("submitting!");

        log("success!");
        oldtask.set_factor(newtask.factor());
        oldtask.set_units(newtask.units());
        oldtask.metadata.name = newtask.metadata.name;
        oldtask.metadata.length = newtask.metadata.length;
        oldtask.metadata.updated = current_time();

        let mut all_tasks = Tasks::load_offline();
        all_tasks.insert(oldtask.clone());
        all_tasks.save_offline();

        navigator.replace(Route::Home {});
        State::refresh();
    };

    let form = rsx! {
        { wtf(TaskType::Cont, Some(&thetask), closure) }
    };

    rsx! {
        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",


            div {
                padding: "20px",

            button {
                class: "emoji-button",
                onclick: move |_| {
                    navigator.replace(Route::Home{});
                },
                img {
                    width: "34px",
                    height: "34px",
                    src: "{back_str()}",
                }
            }
            button {
                class: "emoji-button",
                onclick: move |_| {
                    Tasks::load_offline().delete_task(id);
                    State::refresh();
                    navigator.replace(Route::Home{});
                },
                img {
                    width: "34px",
                    height: "34px",
                    src: "{delete_str()}",
                }

            }

            h3 { "ratio: {ratio}, value: {value}" }

            { form }


               h3 { "{logstr}" }
            }
        }
    }
}

#[component]
fn Edit(id: Uuid) -> Element {
    let task = Tasks::load_offline().get_task(id).unwrap();

    let oldtask = task.clone();
    log(&oldtask);
    let navigator = use_navigator();

    let logstr: Vec<String> = task
        .log
        .time_since(current_time())
        .into_iter()
        .map(|dur| dur_format(dur))
        .collect();
    let logstr = format!("{:?}", logstr);
    let mut logstr = logstr.replace("\"", "");
    logstr.pop();
    logstr.remove(0);

    let closure = move |newtask: Option<Task>| {
        let mut oldtask = task.clone();
        let newtask = newtask.unwrap();

        log("success!");
        oldtask.set_factor(newtask.factor());
        oldtask.set_interval(newtask.interval());
        oldtask.metadata.name = newtask.metadata.name;
        oldtask.metadata.length = newtask.metadata.length;
        oldtask.metadata.updated = current_time();

        let mut all_tasks = Tasks::load_offline();
        all_tasks.insert(oldtask.clone());
        all_tasks.save_offline();

        navigator.replace(Route::Home {});
        State::refresh();
    };

    let form = rsx! {
        { wtf(TaskType::Disc, Some(&oldtask), closure) }
    };

    rsx! {


        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",


            div {
                padding: "20px",

            button {
                class: "emoji-button",
                onclick: move |_| {
                    navigator.replace(Route::Home{});
                },
                img {
                    width: "34px",
                    height: "34px",
                    src: "{back_str()}",
                }
            }
            button {
                class: "emoji-button",
                onclick: move |_| {
                    Tasks::load_offline().delete_task(id);
                    State::refresh();
                    navigator.replace(Route::Home{});
                },
                img {
                    width: "34px",
                    height: "34px",
                    src: "{delete_str()}",
                }
            }

            { form }

       h3 { "{logstr}" }
            }
        }
    }
}

fn dur_format(dur: Duration) -> String {
    if dur > Duration::from_secs(86400) {
        let days = dur.as_secs_f32() / 86400.;
        format!("{:.1}d", days)
    } else if dur > Duration::from_secs(3600) {
        let hrs = dur.as_secs_f32() / 3600.;
        format!("{:.1}h", hrs)
    } else {
        let mins = dur.as_secs_f32() / 60.;
        format!("{:.1}m", mins)
    }
}

fn dur_to_days(dur: Duration) -> String {
    format!("{:.1}", dur.as_secs_f32() / 86400.)
}

fn dur_to_mins(dur: Duration) -> String {
    (dur.as_secs() / 60).to_string()
}

enum TaskType {
    Disc,
    Cont,
}

impl TaskType {
    fn make_task(&self, args: Vec<String>) -> Option<Task> {
        match self {
            Self::Disc => {
                let name = args[0].clone();
                let unit_name = args[1].clone();
                let length = str_as_mins(&args[2])?;
                let daily_units: f32 = args[3].parse().ok()?;
                let value: f32 = args[4].parse().ok()?;
                let logstuff = Contask::new(daily_units, value, unit_name);
                Some(Task::new(name, ValueEq::Cont(logstuff), length))
            }
            Self::Cont => {
                let name = args[0].clone();
                let unit_name = args[1].clone();
                let length = str_as_mins(&args[2])?;
                let daily_units: f32 = args[3].parse().ok()?;
                let value: f32 = args[4].parse().ok()?;
                let logstuff = Contask::new(daily_units, value, unit_name);
                Some(Task::new(name, ValueEq::Cont(logstuff), length))
            }
        }
    }

    fn inputs(&self, task: Option<&Task>) -> Vec<InputThing> {
        match (self, task) {
            (Self::Disc, Some(task)) => {
                let length = format!("{:.2}", task.metadata.length.as_secs_f32() / 60.);
                let interval = format!("{:.2}", task.factor());
                let value = format!("{:.2}", task.factor());

                InputThing::new_w_default(vec![
                    ("name", false, task.metadata.name.as_str()),
                    ("length", true, &length),
                    ("interval", true, &interval),
                    ("value", true, &value),
                ])
            }
            (Self::Cont, Some(task)) => {
                let unit_name = task.unit_name();
                let length = format!("{:.2}", task.metadata.length.as_secs_f32() / 60.);
                let units = format!("{:.2}", task.units());
                let value = format!("{:.2}", task.factor());

                InputThing::new_w_default(vec![
                    ("name", false, task.metadata.name.as_str()),
                    ("unit name", false, &unit_name),
                    ("length", true, &length),
                    ("daily units", true, &units),
                    ("value", true, &value),
                ])
            }
            (Self::Disc, None) => InputThing::news(vec![
                ("name", false),
                ("length", true),
                ("interval", true),
                ("factor", true),
            ]),
            (Self::Cont, None) => InputThing::news(vec![
                ("name", false),
                ("unit name", false),
                ("length", true),
                ("daily units", true),
                ("value", true),
            ]),
        }
    }
}

struct InputThing {
    label: String,
    is_num: bool,
    signal: Signal<String>,
    idx: usize,
}

impl InputThing {
    fn new_def(label: &str, is_num: bool, idx: usize, default: &str) -> Self {
        Self {
            label: label.to_string(),
            is_num,
            signal: Signal::new(String::from(default)),
            idx,
        }
    }

    fn new(label: &str, is_num: bool, idx: usize) -> Self {
        Self::new_def(label, is_num, idx, "")
    }

    fn new_w_default(inp: Vec<(&str, bool, &str)>) -> Vec<Self> {
        let mut v = vec![];

        for (idx, (label, is_num, default)) in inp.into_iter().enumerate() {
            v.push(Self::new_def(label, is_num, idx, default));
        }

        v
    }

    fn news(inp: Vec<(&str, bool)>) -> Vec<Self> {
        let mut v = vec![];

        for (idx, (label, is_num)) in inp.into_iter().enumerate() {
            v.push(Self::new(label, is_num, idx));
        }

        v
    }
}

fn wtf(
    ty: TaskType,
    task: Option<&Task>,
    on_submit: impl Fn(Option<Task>) + 'static + Clone,
) -> Element {
    let inputs = ty.inputs(task);
    let mut signals = vec![];

    for x in &inputs {
        signals.push(x.signal.clone());
    }

    let len = inputs.len();
    rsx! {
        form {
            onsubmit: move |event| {
                let strs = {
                    let data = event.data().values();
                    let mut strs: Vec<String> = vec![];
                    for i in 0..len {
                        strs.push(data.get(&i.to_string()).unwrap().as_value());
                    }
                    strs
                };

                for sig in &mut signals {
                    sig.set(String::new());
                }

                let task = ty.make_task(strs);

                on_submit(task);
            },
            for mut x in inputs {
                div {
                    margin_bottom: "20px",
                    div {
                        display: "flex",
                        flex_direction: "column",
                        justify_content: "space-between",
                        p { "{x.label}" }
                        input {
                            r#type: if x.is_num {"number"} else {"text"},
                            value: (x.signal)(),
                            name: x.idx.to_string(),
                            autocomplete: "off",
                            step: if x.is_num {"any"},
                            oninput: move |event| x.signal.set(event.value()),
                        }
                    }
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

#[component]
fn New() -> Element {
    let state = use_context::<State>();
    let mut selected_value = state.inner.lock().unwrap().tasktype.clone();
    let navigator = navigator();

    log("neww");

    rsx! {
        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",

            div {
                padding: "20px",

                div {
                    display: "flex",
                    flex_direction: "row",
                    button {
                        class: "emoji-button",
                        onclick: move |_| {
                            navigator.replace( Route::Home{} );
                        },

                        img {
                            width: "34px",
                            height: "34px",
                            src: "returning.svg",
                        }
                    }


                    select {
                        class: "dropdown",
                        value: "{selected_value}",
                        onchange: move |e| {
                            log("foo");
                            log(&e);
                            let s = e.value().clone();
                            log("baz");
                            log(&s);
                            *selected_value.write() = s;
                            let x = selected_value.read();
                            log("qux");
                            log(x);
                            log("bar");

                        },
                        option { value: "disc", "Discrete" },
                        option { value: "cont", "Continuous" },
                    }
                }

                if *selected_value.read() == "disc" {
                    { Disc() }
                } else {
                    { Cont() }
                }
            }
        }
    }
}

fn str_as_mins(s: &str) -> Option<Duration> {
    let mins: f32 = s.parse().ok()?;
    Some(Duration::from_secs_f32(mins * 60.))
}

fn str_as_days(s: &str) -> Option<Duration> {
    let mins: f32 = s.parse().ok()?;
    Some(Duration::from_secs_f32(mins * 86400.))
}

#[component]
fn Disc() -> Element {
    let state = use_context::<State>();

    let auth = (*state.inner.lock().unwrap().auth_status.clone().read()).clone();

    let navigator = navigator();

    log("neww");

    let closure = move |task: Option<Task>| {
        let task = task.unwrap();

        if let Some(user) = auth.user() {
            let future = send_task_to_firestore(user.uid.clone(), &task);
            wasm_bindgen_futures::spawn_local(async {
                future.await.unwrap();
            });
        }

        let mut the_tasks = Tasks::load_offline();
        the_tasks.insert(task);
        the_tasks.save_offline();
        navigator.replace(Route::Home {});
        State::refresh();
    };

    rsx! {
        { wtf(TaskType::Disc, None, closure) }
    }
}

#[component]
fn Cont() -> Element {
    let state = use_context::<State>();

    let auth = (*state.inner.lock().unwrap().auth_status.clone().read()).clone();

    let navigator = navigator();

    let closure = move |task: Option<Task>| {
        let task = task.unwrap();

        log_to_console(&task);
        if let Some(user) = auth.user() {
            let future = send_task_to_firestore(user.uid.clone(), &task);
            wasm_bindgen_futures::spawn_local(async {
                future.await.unwrap();
            });
        }

        let mut the_tasks = Tasks::load_offline();
        the_tasks.insert(task);
        the_tasks.save_offline();
        navigator.replace(Route::Home {});
        State::refresh();
    };

    rsx! {
           { wtf(TaskType::Cont, None, closure) }
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
    let valueform = format!("{:.2}", value_stuff);
    let mut auth = state.inner.lock().unwrap().auth_status.clone();
    let is_syncing = state.inner.lock().unwrap().is_syncing.clone();

    let navigator = use_navigator();

    rsx! {
        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",
            flex_direction: "column",


            div {
                padding: "20px",

                div {
                    display: "flex",
                    flex_direction: "row",
                    margin_bottom: "20px",

                    if (*auth.read()).is_authed(){
                        button {
                            class: "emoji-button",
                            onclick: move |_| {
                                sync_tasks(is_syncing.clone());
                                tasks.set(task_props());
                                value_stuff.set(tot_value_since());
                            },

                            if is_syncing() {
                                img {
                                    width: "34px",
                                    height: "34px",
                                    src: "sync.svg",
                                }
                            } else {
                                img {
                                    width: "34px",
                                    height: "34px",
                                    src: "sync.svg",
                                }
                            }

                        }
                    } else {
                        button {
                            class: "emoji-button",
                            onclick: move |_| {
                                let promise = signInWithGoogle();
                                let future = wasm_bindgen_futures::JsFuture::from(promise);
                                wasm_bindgen_futures::spawn_local(async move{
                                    let val = future.await.unwrap();
                                    let user = AuthUser::from_jsvalue(val);
                                    *auth.write() = AuthStatus::Auth(user);
                                });


                            },

                            img {
                                width: "34px",
                                height: "34px",
                                src: "signin.svg",
                            }
                        }
                    }


                    button {
                        class: "emoji-button",
                        onclick: move |_| {
                            navigator.replace(Route::New{});
                        },

                        img {
                            width: "34px",
                            height: "34px",
                            src: "addnew.svg",
                        }
                    }

                    button {
                        class: "emoji-button",
                        onclick: move |_| {
                            tasks.set(task_props());
                            value_stuff.set(tot_value_since());
                        },
                        "🔄"
                    }

                }

                div {
                    margin_bottom: "50px",
                    "xValue 24h: {valueform}"
                }

                div {
                    display: "flex",
                    flex_direction: "column",

                    for task in tasks() {

                        div {
                            display: "flex",
                            flex_direction: "row",
                            margin_bottom: "10px",

                            div {
                                button {
                                    class: "emoji-button",
                                    font_size: "1.0em",

                                    margin_right: "5px",
                                    onclick: move |_| {
                                        log_to_console(&task.name);
                                        if task.disc {
                                            Tasks::load_offline().do_task(task.id, 1.0);
                                        } else {
                                            navigator.replace(Route::Units{id: task.id});
                                        };
                                        State::refresh();
                                    },
                                    "✅"
                                }
                            }
                            span {

                                margin_right: "5px",
                                "{task.priority}" }


                            if task.disc {
                                Link { to: Route::Edit {id: task.id}, "{task.name}" }
                            } else {
                                Link { to: Route::Editcont {id: task.id}, "{task.name}" }

                            }
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
    Cont(Contask),
}

impl ValueEq {
    fn value(&self, logs: &TaskLog, created: UnixTime, current_time: UnixTime) -> f32 {
        let last_completed = logs.last_completed().unwrap_or(created);
        let time_since = current_time - last_completed;

        match self {
            Self::Const(f) => *f,
            Self::Cont(c) => c.value(logs, current_time),
            Self::Log(log) => log.value(time_since),
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

    fn do_task(&mut self, id: Uuid, units: f32) {
        let mut task = self.get_task(id).unwrap();
        task.do_task(units);
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
    fn new(name: impl Into<String>, equation: ValueEq, length: Duration) -> Self {
        let time = current_time();
        Self {
            name: name.into(),
            created: time,
            updated: time,
            value: equation,
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
    send_up: Vec<LogRecord>,
    save: TaskLog,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
struct LogRecord {
    time: UnixTime,
    units: f32,
}

impl LogRecord {
    fn new(time: UnixTime, units: f32) -> Self {
        Self { time, units }
    }

    fn new_current(units: f32) -> Self {
        let time = current_time();
        Self::new(time, units)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TaskLog(Vec<LogRecord>);

impl TaskLog {
    fn new(&mut self, record: LogRecord) {
        if !self.0.contains(&record) {
            self.0.push(record);
        }
    }

    fn time_since(&self, time: UnixTime) -> Vec<Duration> {
        let mut vec = vec![];

        for log in &self.0 {
            vec.push(time - log.time);
        }

        vec
    }

    fn last_completed(&self) -> Option<UnixTime> {
        self.0.last().copied().map(|rec| rec.time)
    }

    fn newlol(mut logs: Vec<LogRecord>) -> Self {
        logs.sort_by_key(|log| log.time);
        Self(logs)
    }

    fn sync(from_online: Self, from_offline: Self) -> LogSyncRes {
        let mut res = LogSyncRes::default();
        let mut send_up = vec![];
        let mut save = vec![];

        for unix in from_offline.0 {
            if !from_online.0.contains(&unix) {
                send_up.push(unix);
            }

            if !save.contains(&unix) {
                save.push(unix);
            }
        }

        for unix in from_online.0 {
            if !save.contains(&unix) {
                save.push(unix);
            }
        }

        send_up.sort_by_key(|rec| rec.time);

        res.send_up = send_up;
        res.save = Self::newlol(save);

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
            log(("X@@@: ", &x));

            let foo = x.as_object().unwrap().get("units").unwrap().as_str();

            let units: f32 = match foo {
                Some(s) => s.parse().unwrap(),
                None => 1.,
            };

            let log = LogRecord::new(ts, units);
            logs.push(log);
        }

        logs.sort_by_key(|log| log.time);
        Self(logs)
    }

    fn merge(&mut self, other: Self) {
        let mut merged = vec![];

        for log in &self.0 {
            if !merged.contains(log) {
                merged.push(*log);
            }
        }

        for log in &other.0 {
            if !merged.contains(log) {
                merged.push(*log);
            }
        }

        merged.sort_by_key(|mrg| mrg.time);

        *self = Self(merged);
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
    fn new(name: impl Into<String>, equation: ValueEq, length: Duration) -> Self {
        Self {
            id: Uuid::new_v4(),
            metadata: MetaData::new(name, equation, length),
            log: TaskLog::default(),
        }
    }

    fn is_disc(&self) -> bool {
        match self.metadata.value {
            ValueEq::Log(_) => true,
            ValueEq::Cont(_) => false,
            ValueEq::Const(_) => false,
        }
    }

    fn set_units(&mut self, units: f32) {
        if let ValueEq::Cont(ref mut l) = &mut self.metadata.value {
            l.daily_units = units;
            return;
        }

        panic!();
    }
    fn set_interval(&mut self, interval: Duration) {
        if let ValueEq::Log(ref mut l) = &mut self.metadata.value {
            l.interval = interval;
            return;
        }

        panic!();
    }

    fn set_factor(&mut self, factor: f32) {
        match &mut self.metadata.value {
            ValueEq::Log(x) => x.factor = factor,
            ValueEq::Cont(x) => x.factor = factor,
            ValueEq::Const(x) => *x = factor,
        };
    }

    fn factor(&self) -> f32 {
        match &self.metadata.value {
            ValueEq::Log(x) => x.factor,
            ValueEq::Cont(x) => x.factor,
            ValueEq::Const(x) => *x,
        }
    }

    fn ratio(&self) -> f32 {
        if let ValueEq::Cont(l) = &self.metadata.value {
            return l.ratio(&self.log, current_time());
        }

        panic!();
    }
    fn unit_name(&self) -> String {
        if let ValueEq::Cont(l) = &self.metadata.value {
            return l.unit_name.clone().unwrap_or("units".to_string());
        }

        panic!();
    }
    fn units(&self) -> f32 {
        if let ValueEq::Cont(l) = &self.metadata.value {
            return l.daily_units;
        }

        panic!();
    }

    fn interval(&self) -> Duration {
        if let ValueEq::Log(l) = &self.metadata.value {
            return l.interval;
        }

        panic!();
    }

    fn do_task(&mut self, units: f32) {
        let record = LogRecord::new_current(units);
        self.log.new(record);
        block_on(self.log.save_offline(self.id));

        let state = use_context::<State>();
        if let Some(user) = state.auth_user() {
            let future = add_task_log_to_firestore(user.uid, self.id, record);
            wasm_bindgen_futures::spawn_local(async {
                match future.await {
                    Ok(_) => web_sys::console::log_1(&JsValue::from_str("Log added successfully")),
                    Err(e) => web_sys::console::log_1(&e),
                }
            });
        }
    }

    /// Hourly wage
    fn priority(&self) -> f32 {
        let now = current_time();
        let val = self
            .metadata
            .value
            .value(&self.log, self.metadata.created, now);

        let hour_length = self.metadata.length.as_secs_f32() / 3600.;
        val / hour_length
    }

    // Value accrued after 'dur'.
    fn value_since(&self, cutoff: UnixTime) -> f32 {
        let mut value_accrued = 0.;
        let tasklog = self.log.clone();

        let mut inner = vec![];
        for log in &tasklog.0 {
            let time = log.time;
            if time > cutoff {
                let value = self.metadata.value.value(
                    &TaskLog::newlol(inner.clone()),
                    self.metadata.created,
                    time,
                );

                value_accrued += value;
            }
            inner.push(log.clone());
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

pub fn log(message: impl std::fmt::Debug) -> impl std::fmt::Debug {
    log_to_console(&message);
    message
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

async fn fetch_tasks() -> HashMap<Uuid, Task> {
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

#[derive(Props, PartialEq, Clone)]
struct TaskProp {
    name: String,
    priority: String,
    id: Uuid,
    disc: bool,
}

impl TaskProp {
    fn from_task(task: &Task) -> Self {
        Self {
            name: task.metadata.name.clone(),
            priority: format!("{:.2}", task.priority()),
            id: task.id,
            disc: task.is_disc(),
        }
    }
}

fn back_str() -> &'static str {
    include_str!("../assets/return.svg")
}

fn delete_str() -> &'static str {
    include_str!("../assets/delete.svg")
}
