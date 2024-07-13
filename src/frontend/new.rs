#![allow(non_snake_case)]

use super::*;

use crate::firebase;
use crate::task::{Task, Tasks};
use crate::State;

#[component]
pub fn New() -> Element {
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

#[component]
pub fn Disc() -> Element {
    let state = use_context::<State>();

    let auth = (*state.inner.lock().unwrap().auth_status.clone().read()).clone();

    let navigator = navigator();

    log("neww");

    let closure = move |task: Option<Task>| {
        let task = task.unwrap();

        if let Some(user) = auth.user() {
            let future = firebase::send_task_to_firestore(user.uid.clone(), &task);
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
pub fn Cont() -> Element {
    let state = use_context::<State>();

    let auth = (*state.inner.lock().unwrap().auth_status.clone().read()).clone();

    let navigator = navigator();

    let closure = move |task: Option<Task>| {
        let task = task.unwrap();

        log_to_console(&task);
        if let Some(user) = auth.user() {
            let future = firebase::send_task_to_firestore(user.uid.clone(), &task);
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
