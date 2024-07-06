#![allow(non_snake_case)]

use dioxus::prelude::*;
use js_sys::Date;
use std::collections::HashMap;
use std::time::Duration;
use tracing::Level;
use wasm_bindgen::prelude::*;
use web_sys::console;

type UnixTime = Duration;

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

use futures::executor::block_on;

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
                    let mut tasks = load_tasks();
                    tasks.push(task);
                    save_tasks(tasks);
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
                        min: "0",
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
                        min: "0",
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
                        min: "0",
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
                    div { "name: {task.name}   hourly wage: {task.priority}" }
                }
            }
        }
    }
}

fn min_dur(mins: f32) -> Duration {
    let secs = mins * 60.;
    Duration::from_secs_f32(secs)
}

fn day_dur(days: f32) -> Duration {
    let secs = days * 86400.;
    Duration::from_secs_f32(secs)
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
            slope: Slope::Normal.factor(),
        }
    }

    fn common_factor(unity: f32, twonity: f32) -> f32 {
        let ratio = twonity / unity;
        (ratio - 2.) / unity
    }

    fn ab(unity: f32, twonity: f32) -> (f32, f32) {
        let common = Self::common_factor(unity, twonity);
        dbg!(common);
        let a = common;
        let b = unity * common + 1.;
        dbg!(b);
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
}

impl ValueEq {
    fn value(&self, t: Duration) -> f32 {
        match self {
            Self::Log(log) => log.value(t),
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

#[derive(Debug)]
enum Slope {
    Linear,
    Normal,
    Steep,
}

impl Slope {
    fn factor(&self) -> f32 {
        let e = std::f32::consts::E;
        match self {
            Slope::Linear => e - 0.5,
            Slope::Normal => e + 1.,
            Slope::Steep => e + 100.,
        }
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

    serde_json::from_str(eval.as_str().unwrap()).unwrap()
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
    priority: f32,
    created: UnixTime,
}

impl TaskProp {
    fn from_task(task: &Task) -> Self {
        Self {
            name: task.name.clone(),
            priority: task.priority(),
            created: task.created,
        }
    }
}

/*
#[derive(Debug, PartialEq, Clone)]
pub struct Message {
    pub origin: String,
    pub content: String,
}

impl Message {
    pub fn new(origin: String, content: String) -> Self {
        Self { origin, content }
    }
}

#[derive(Props, PartialEq, Clone)]
struct MessageProps {
    class: &'static str,
    sender: &'static str,
    content: String,
}

fn Message(msg: MessageProps) -> Element {
    rsx!(
        div {
            class: "{msg.class}",
            strong { "{msg.sender}" }
            span { "{msg.content}" }
        }
    )
}

#[derive(Props, PartialEq, Clone)]
pub struct MessageListProps {
    messages: Vec<Message>,
}

pub fn MessageList(mut msgs: MessageListProps) -> Element {
    msgs.messages.reverse();
    rsx!(
        div {
            class: "message-list",
            display: "flex",
            flex_direction: "column-reverse",
            for msg in msgs.messages{
                Message {class: msg.origin.class(), sender: msg.origin.str(), content: msg.content}
            }
        }
    )
}
*/
