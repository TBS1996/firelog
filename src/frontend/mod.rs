#![allow(non_snake_case)]

use crate::cache;
use crate::task::{Contask, LogPriority, Task, Tasks, ValueEq};
use crate::utils;
use crate::State;
use dioxus::prelude::*;
use std::time::Duration;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::console;

mod about;
mod edit;
mod home;
mod new;
mod stats;
mod units;

use about::*;
use edit::*;
use home::*;
use new::*;
use stats::*;
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
        cache::save_uid(&uid);

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
    #[layout(Wrapper)]
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
    #[route("/stats/:id")]
    Stats { id: Uuid },
}

#[component]
fn Wrapper() -> Element {
    rsx! {
        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",
            width: "100vw",
            flex_direction: "column",

            div {
                width: "300px",

                Outlet::<Route> {}

                div {
                    display: "flex",
                    justify_content: "center",
                    margin_top: "50px",
                    { footer() }
                }
            }
        }
    }
}

pub fn footer() -> Element {
    rsx! {
        div {
            Link {
                to: Route::About {},
                "about"
            }
            a {
                margin_left: "20px",
                href: "https://github.com/tbs1996/firelog/issues",
                target: "_blank",
                "feedback"
            }
        }
    }
}

impl Route {
    fn has_args(&self) -> bool {
        match self {
            Self::Home { .. } => false,
            Self::New { .. } => false,
            Self::Units { .. } => true,
            Self::About { .. } => false,
            Self::Edit { .. } => true,
            Self::Editcont { .. } => true,
            Self::Stats { .. } => true,
        }
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
            display: "flex",
            align_items: "center",
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
                    margin_bottom: "1px",
                    width: "200px",
                    div {
                        display: "flex",
                        flex_direction: "column",
                        width: "200px",
                        justify_content: "space-between",
                        div {
                            display: "flex",
                            flex_direction: "row",
                            align_items: "center",
                            p {
                                text_align: "center",
                                width: "200px",
                                margin_bottom: "5px",
                                "{x.label}"
                            }
                            if let Some(txt) = x.tooltip {
                                    div {
                                        margin_left: "5px",
                                        vertical_align: "middle",
                                        { qmtt(&txt, 20) }
                                    }
                            }
                        }
                        input {
                            r#type: if x.is_num {"number"} else {"text"},
                            value: (x.signal)(),
                            width: "200px",
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
                margin_top: "30px",
                width: "200px",
                if task.is_some() {
                    "Update task"
                } else {
                    "Create task"
                }
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
    value: String,
    id: Uuid,
    disc: bool,
}

impl TaskProp {
    fn from_task(task: &Task) -> Self {
        Self {
            name: task.metadata.name.clone(),
            priority: utils::format_float(task.priority()),
            id: task.id,
            disc: task.is_disc(),
            value: utils::format_float(task.value()),
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

pub fn tot_value_since(since: Duration) -> f32 {
    let time = utils::current_time() - since;
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
                let interval = utils::str_as_days(&args[1])?;
                let value: f32 = args[2].parse().ok()?;
                let length = utils::str_as_mins(&args[3])?;

                let logstuff = LogPriority::new(value, interval);
                Some(Task::new(name, ValueEq::Log(logstuff), length))
            }
            Self::Cont => {
                let name = args[0].clone();
                let unit_name = args[1].clone();
                let length = utils::str_as_mins(&args[2])?;
                let daily_units: f32 = args[3].parse().ok()?;
                let value: f32 = args[4].parse().ok()?;
                let value = value / daily_units;
                let logstuff = Contask::new(daily_units, value, unit_name);
                Some(Task::new(name, ValueEq::Cont(logstuff), length))
            }
        }
    }

    fn inputs(&self, task: Option<&Task>) -> Vec<InputThing> {
        match (self, task) {
            (Self::Disc, Some(task)) => {
                let length = format!("{:.2}", task.metadata.length.as_secs_f32() / 60.);
                let interval = format!("{:.2}", task.interval().as_secs_f32() / 86400.);
                let value = format!("{:.2}", task.factor());

                InputThing::new_w_default(vec![
                    ("name", false, task.metadata.name.as_str(), None),
                    ("interval", true, &interval, None),
                    ("value", true, &value, None),
                    ("length", true, &length, None),
                ])
            }
            (Self::Cont, Some(task)) => {
                let unit_name = task.unit_name();
                let length = format!("{:.2}", task.metadata.length.as_secs_f32() / 60.);
                let units = format!("{:.2}", task.units());
                let value = format!("{:.2}", task.factor() * task.units());

                InputThing::new_w_default(vec![
                    ("name", false, task.metadata.name.as_str(), None),
                    ("unit name", false, &unit_name, None),
                    ("length", true, &length, None),
                    ("daily units", true, &units, None),
                    ("value", true, &value, None),
                ])
            }
            (Self::Disc, None) => InputThing::new_w_default(vec![
                ("name", false, "", Some("name of task")),
                ("interval", true, "", Some("how often you'd do the task (in days)")),
                ("value", true, "", Some("How much you'd pay to have task done after 'interval' days. If you couldn't do it yourself")),
                ("length", true, "", Some("minutes to complete the task")),
            ]),
            (Self::Cont, None) => InputThing::new_w_default(vec![
                ("name", false, "", Some("name of task")),
                ("unit name", false, "", Some("name of unit, e.g. minutes, pages, kilometers")),
                ("length", true, "", Some("time to finish one unit")),
                ("daily units", true, "", Some("Approx how many units you want to do per day")),
                ("value", true, "", Some("How much you'd pay to have all daily units done if you couldn't do them yourself")),
            ]),
        }
    }
}

struct InputThing {
    label: String,
    is_num: bool,
    signal: Signal<String>,
    idx: usize,
    tooltip: Option<String>,
}

impl InputThing {
    fn new_full(
        label: &str,
        is_num: bool,
        idx: usize,
        default: &str,
        tooltip: Option<String>,
    ) -> Self {
        Self {
            label: label.to_string(),
            is_num,
            signal: Signal::new(String::from(default)),
            idx,
            tooltip,
        }
    }

    fn new_w_default(inp: Vec<(&str, bool, &str, Option<&str>)>) -> Vec<Self> {
        let mut v = vec![];

        for (idx, (label, is_num, default, tooltip)) in inp.into_iter().enumerate() {
            v.push(Self::new_full(
                label,
                is_num,
                idx,
                default,
                tooltip.map(ToOwned::to_owned),
            ));
        }

        v
    }
}

fn qmark_str() -> &'static str {
    include_str!("../../assets/qmark64")
}

pub fn tooltip(main_text: &str, tooltip: &str, text_size: f32) -> Element {
    let text_size = format!("{}em", text_size);
    rsx! {
        div {
            class: "tooltip-container",
            color: "#666",
            "{main_text}",
            div {
                class: "tooltip-text",
                font_size: text_size,
                z_index: "5000",
                color: "white",
                "{tooltip}"
            }
        }
    }
}

pub fn tooltip_image(src: &str, msg: &str, img_size: usize, text_size: f32) -> Element {
    let size = format!("{}px", img_size.to_string());
    let text_size = format!("{}em", text_size);

    rsx! {
        div {
            class: "tooltip-container",
            img {
                width: "{size}",
                height: "{size}",
                src: "{src}",
            }
            div {
                class: "tooltip-text",
                font_size: text_size,
                color: "white",
                "{msg}"
            }
        }
    }
}

pub fn qmtt(msg: &str, size: usize) -> Element {
    let src = if use_route::<Route>().has_args() {
        qmark_str()
    } else {
        "questionmark.svg"
    };

    tooltip_image(src, msg, size, 1.2)
}
