#![allow(non_snake_case)]

use super::*;

use crate::firebase;
use crate::sync::sync_tasks;
use crate::task::Tasks;
use crate::State;

#[component]
pub fn Home() -> Element {
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
                                    src: "hourglass.svg",
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
                                let promise = firebase::signInWithGoogle();
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
                    "Value 24h: {valueform}"
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
