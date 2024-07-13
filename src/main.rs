use crate::task::MetaData;
use dioxus::prelude::*;
use std::sync::{Arc, Mutex};
use tracing::Level;

mod cache;
mod firebase;
mod frontend;
mod sync;
mod task;
mod utils;

use crate::frontend::App;
use crate::frontend::TaskProp;
use crate::frontend::*;
use crate::task::Task;

fn main() {
    dioxus_logger::init(Level::INFO).expect("failed to init logger");
    launch(App);
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
