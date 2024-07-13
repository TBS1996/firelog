#![allow(non_snake_case)]

use crate::cache;
use crate::firebase;
use crate::task::{Contask, LogPriority, Task, Tasks, ValueEq};
use crate::utils;
use crate::State;
use dioxus::prelude::*;
use futures::executor::block_on;
use std::time::Duration;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::console;

mod about;
mod edit;
mod home;
mod new;
mod units;

use about::*;
use edit::*;
use home::*;
use new::*;
use units::*;

pub fn App() -> Element {
    use_context_provider(State::load);
    rsx! {
        Router::<Route> {}
    }
}

#[derive(Default, Clone)]
pub struct AuthUser {
    pub uid: String,
}

impl AuthUser {
    pub fn from_jsvalue(val: wasm_bindgen::JsValue) -> Self {
        use gloo_utils::format::JsValueSerdeExt;

        let wtf: serde_json::Value = JsValueSerdeExt::into_serde(&val).unwrap();
        let obj = wtf.as_object().unwrap();
        let uid = obj.get("uid").unwrap().as_str().unwrap().to_owned();
        cache::save("uid", &uid);

        Self { uid }
    }
}

#[derive(Default, Clone)]
pub enum AuthStatus {
    Auth(AuthUser),
    #[default]
    Nope,
}

impl AuthStatus {
    pub fn user(&self) -> Option<AuthUser> {
        match self {
            Self::Auth(user) => Some(user.clone()),
            Self::Nope => None,
        }
    }

    pub fn is_authed(&self) -> bool {
        match self {
            Self::Nope => false,
            Self::Auth(_) => true,
        }
    }
}

#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
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

#[allow(dead_code)]
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

pub fn log(message: impl std::fmt::Debug) -> impl std::fmt::Debug {
    log_to_console(&message);
    message
}

pub fn log_to_console(message: impl std::fmt::Debug) {
    let message = format!("{:?}", message);
    console::log_1(&JsValue::from_str(&message));
}

#[derive(Props, PartialEq, Clone)]
pub struct TaskProp {
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
    include_str!("../../assets/return.svg")
}

fn delete_str() -> &'static str {
    include_str!("../../assets/delete.svg")
}

pub fn task_props() -> Vec<TaskProp> {
    let mut tasks = Tasks::load_offline();
    tasks.prune_deleted();

    let tasks = tasks.to_vec_sorted();

    let tasks: Vec<TaskProp> = tasks.iter().map(|task| TaskProp::from_task(task)).collect();
    tasks
}

pub fn tot_value_since() -> f32 {
    let time = utils::current_time() - Duration::from_secs(86400);
    let mut value = 0.;
    let mut tasks = Tasks::load_offline();
    tasks.prune_deleted();
    let tasks = tasks.to_vec_sorted();

    for task in tasks {
        value += task.value_since(time);
    }

    value
}

pub enum TaskType {
    Disc,
    Cont,
}

impl TaskType {
    fn make_task(&self, args: Vec<String>) -> Option<Task> {
        match self {
            Self::Disc => {
                let name = args[0].clone();
                let length = utils::str_as_mins(&args[2])?;
                let interval = utils::str_as_days(&args[2])?;
                let value: f32 = args[4].parse().ok()?;

                let logstuff = LogPriority::new(value, interval);
                Some(Task::new(name, ValueEq::Log(logstuff), length))
            }
            Self::Cont => {
                let name = args[0].clone();
                let unit_name = args[1].clone();
                let length = utils::str_as_mins(&args[2])?;
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
