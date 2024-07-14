use crate::task::MetaData;
use dioxus::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
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
        let selected_value = state.inner.lock().unwrap().selected_dur.clone();
        let x = selected_value.read();
        let dur = utils::value_since(&x);
        tasks.set(task_props());
        value_stuff.set(tot_value_since(dur));
    }
}

fn try_persistent_signed_in(mut auth: Signal<AuthStatus>) {
    let future = firebase::is_authed();
    wasm_bindgen_futures::spawn_local(async move {
        let x = future.await.unwrap_or_default();
        let y = x.as_bool().unwrap();
        if y {
            if let Some(uid) = cache::load_uid().await {
                let bruh = AuthStatus::Auth(AuthUser { uid });
                auth.set(bruh);
            }
        }
    });
}

#[derive(Clone, Default)]
struct StateInner {
    auth_status: Signal<AuthStatus>,
    tasktype: Signal<String>,
    tasks: Signal<Vec<TaskProp>>,
    value_stuff: Signal<f32>,
    is_syncing: Signal<bool>,
    selected_dur: Signal<String>,
}

impl StateInner {
    fn load() -> Self {
        let auth_status = Signal::new(AuthStatus::Nope);
        try_persistent_signed_in(auth_status.clone());

        Self {
            auth_status,
            tasktype: Signal::new(String::from("disc")),
            tasks: Signal::new(task_props()),
            value_stuff: Signal::new(tot_value_since(Duration::from_secs(86400))),
            is_syncing: Signal::new(false),
            selected_dur: Signal::new(String::from("1")),
        }
    }
}
