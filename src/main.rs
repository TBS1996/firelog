#![allow(non_snake_case)]

use dioxus::prelude::*;
use futures::executor::block_on;
use js_sys::Date;
use std::collections::HashMap;
use std::time::Duration;
use tracing::Level;
use wasm_bindgen::prelude::*;
use web_sys::console;

type UnixTime = Duration;

const DEFAULT_SLOPE: f32 = std::f32::consts::E + 1.;

#[derive(Clone, Routable, Debug, PartialEq)]
enum Route {
    #[route("/")]
    Home {},
}

fn main() {
    // Init logger
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    launch(App);
}

fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

fn task_props() -> Vec<TaskProp> {
    let mut tasks = load_tasks();
    tasks.sort_by_key(|task| (task.priority() * 1000.) as u32);
    tasks.reverse();
    let tasks: Vec<TaskProp> = tasks.iter().map(|task| TaskProp::from_task(task)).collect();
    tasks
}

fn tot_value_since() -> f32 {
    let dur = Duration::from_secs(86400);
    let mut value = 0.;
    let tasks = load_tasks();

    for task in tasks {
        value += task.value_since(dur);
    }

    value
}

#[component]
fn Home() -> Element {
    let mut name = use_signal(|| String::new());
    let mut length = use_signal(|| String::new());
    let mut interval = use_signal(|| String::new());
    let mut factor = use_signal(|| String::new());
    let mut tasks = use_signal(|| task_props());
    let mut value_stuff = use_signal(|| tot_value_since());

    rsx! {

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
                    let mut the_tasks = load_tasks();
                    the_tasks.push(task);
                    save_tasks(the_tasks);
                    tasks.set(task_props());
                    value_stuff.set(tot_value_since());
                }

            },
            div {
                class: "input-group",
                display: "flex",
                flex_direction: "column",

                div {
                    flex_direction: "row",
                    { "name" }
                    input {
                        r#type: "text",
                        name: "name",
                        value: name(),
                        autocomplete: "off",
                        oninput: move |event| name.set(event.value()),
                    }
                }
                div {
                    flex_direction: "row",
                    { "length" }
                    input {
                        r#type: "number",
                        min: "1",
                        required: true,
                        step: "any",
                        name: "length",
                        value: length(),
                        autocomplete: "off",
                        oninput: move |event| length.set(event.value()),
                    }
                }
                div {
                    flex_direction: "row",
                    { "interval" }
                    input {
                        r#type: "number",
                        min: "0.01",
                        required: true,
                        step: "any",
                        name: "interval",
                        value: interval(),
                        autocomplete: "off",
                        oninput: move |event| interval.set(event.value()),
                    }
                }
                div {
                    flex_direction: "row",
                    { "value" }
                    input {
                        r#type: "number",
                        required: true,
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

        div {
            display: "flex",
            flex_direction: "row",

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
                        onclick: move |_| {
                            Task::delete_task(task.created);
                            tasks.set(task_props());
                            value_stuff.set(tot_value_since());
                        },
                        "❌"
                    }
                    button {
                        onclick: move |_| {
                            log_to_console(&task.name);
                            Task::do_task(task.created);
                            tasks.set(task_props());
                            value_stuff.set(tot_value_since());
                        },
                        "✅"
                    }
                    div { "{task.priority} {task.name}" }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    name: String,
    value: ValueEq,
    length: Duration,
    created: UnixTime,
    log: Vec<UnixTime>,
}

impl Task {
    fn new(name: impl Into<String>, equation: LogPriority, length: Duration) -> Self {
        Self {
            name: name.into(),
            created: current_time(),
            log: vec![],
            value: ValueEq::Log(equation),
            length,
        }
    }

    fn delete_task(id: UnixTime) {
        let mut tasks = load_tasks();
        tasks.retain(|task| task.created != id);
        save_tasks(tasks);
    }

    fn do_task(id: UnixTime) {
        let mut tasks = load_tasks();
        for task in tasks.iter_mut() {
            if task.created == id {
                task.log.push(current_time());
            }
        }
        save_tasks(tasks);
    }

    fn from_form(form: HashMap<String, FormValue>) -> Option<Self> {
        let name = form.get("name")?.as_value();

        let factor: f32 = form.get("factor")?.as_value().parse().ok()?;

        let interval = {
            let interval = form.get("interval")?.as_value();
            let days: f32 = interval.parse().ok()?;
            Duration::from_secs_f32(days * 86400.)
        };

        let length = {
            let length = form.get("length")?.as_value();
            let mins: f32 = length.parse().ok()?;
            Duration::from_secs_f32(mins * 60.)
        };

        let logstuff = LogPriority::new(factor, interval);

        Some(Self::new(name, logstuff, length))
    }

    /// Hourly wage
    fn priority(&self) -> f32 {
        let t = self.time_since_last_completion();
        log_to_console(("time since", &t));

        let val = self.value.value(t);
        let hour_length = self.length.as_secs_f32() / 3600.;
        val / hour_length
    }

    fn time_since_last_completion(&self) -> Duration {
        current_time() - self.last_completed()
    }

    fn last_completed(&self) -> UnixTime {
        log_to_console(("log: ", &self.log));
        log_to_console(("created: ", &self.created));
        let last = match self.log.last() {
            Some(time) => *time,
            None => self.created,
        };
        log_to_console(("last completed: ", &last));
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
    log_to_console(&seconds_since_epoch);
    UnixTime::from_secs(seconds_since_epoch)
}

pub trait Value {
    fn value(&self, t: Duration) -> f32;
}

impl Value for LogPriority {
    fn value(&self, t: Duration) -> f32 {
        let t = t.as_secs_f32() / 86400.;
        let t1 = self.interval.as_secs_f32() / 86400.;
        let t2 = t1 * self.slope;

        let (a, b) = Self::ab(t1, t2);
        (a * t + 1.).log(b) * self.factor
    }
}

pub fn log_to_console(message: impl std::fmt::Debug) {
    let message = format!("{:?}", message);
    console::log_1(&JsValue::from_str(&message));
}

fn load_tasks() -> Vec<Task> {
    block_on(fetch_tasks())
}

async fn fetch_tasks() -> Vec<Task> {
    let eval = eval(
        r#"
        let id = localStorage.getItem('tasks');
        if (id) {
            dioxus.send(id);
        } else {
            dioxus.send(null);
        }
        "#,
    )
    .recv()
    .await
    .unwrap();

    match eval.as_str() {
        Some(str) => serde_json::from_str(str).unwrap_or_default(),
        None => vec![],
    }
}

fn save_tasks(tasks: Vec<Task>) {
    let s = serde_json::to_string(&tasks).unwrap();

    let script = format!("localStorage.setItem('tasks', '{}');", s);
    eval(&script);
    log_to_console("storing user_id in local storage");
}

#[derive(Props, PartialEq, Clone)]
struct TaskProp {
    name: String,
    priority: String,
    created: UnixTime,
}

impl TaskProp {
    fn from_task(task: &Task) -> Self {
        Self {
            name: task.name.clone(),
            priority: format!("{:.2}", task.priority()),
            created: task.created,
        }
    }
}
