#![allow(non_snake_case)]

use super::*;

use crate::firebase;
use crate::sync::sync_tasks;
use crate::task::Tasks;
use crate::utils;
use crate::State;

#[component]
pub fn Home() -> Element {
    let state = use_context::<State>();

    let mut tasks = state.inner.lock().unwrap().tasks.clone();
    let mut value_stuff = state.inner.lock().unwrap().value_stuff.clone();
    let valueform = format!("💸{}", utils::format_float(value_stuff()));
    let mut auth = state.inner.lock().unwrap().auth_status.clone();
    let is_syncing = state.inner.lock().unwrap().is_syncing.clone();
    let mut selected_value = state.inner.lock().unwrap().selected_dur.clone();

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
                    width: "250px",
                    justify_content: "space-between",

                    if (*auth.read()).is_authed(){
                        button {
                            class: "emoji-button",
                            onclick: move |_| {
                                sync_tasks(is_syncing.clone());
                                tasks.set(task_props());
                                let x = selected_value.read();
                                let dur = utils::value_since(&x);
                                value_stuff.set(tot_value_since(dur));
                            },

                            if is_syncing() {

                                { tooltip_image("hourglass.svg", "syncing in progress", 34, 0.4) }

                            } else {
                                { tooltip_image("sync.svg", "initiate sync", 34, 0.4) }
                            }

                        }
                    } else {
                        button {
                            class: "emoji-button",
                            onclick: move |_| {
                                let future = firebase::sign_in_google();
                                wasm_bindgen_futures::spawn_local(async move{
                                    let val = future.await.unwrap();
                                    let user = AuthUser::from_jsvalue(val);
                                    *auth.write() = AuthStatus::Auth(user);
                                });
                            },


                            { tooltip_image("signin.svg", "sign in", 34, 0.4) }

                        }
                    }

                    button {
                        class: "emoji-button",
                        onclick: move |_| {
                            navigator.replace(Route::New{});
                        },

                        { tooltip_image("addnew.svg", "new task", 34, 0.4) }

                    }

                    button {
                        class: "emoji-button",
                        onclick: move |_| {
                            State::refresh();
                        },
                        { tooltip("🔄", "update", 0.4) }
                    }
                }

                div {
                    display: "flex",
                    flex_direction: "row",

                    div {
                        class: "tooltip-container",
                        font_size: "2.0em",
                        color: "#666",
                        "{valueform}",
                        div {
                            class: "tooltip-text",
                            z_index: "5000",
                            font_size: "0.5em",
                            "how much money you've 'earned'"
                        }
                    }


                    select {
                        margin_left: "20px",
                        class: "dropdown",
                        value: "{selected_value}",
                        width: "70px",
                        onchange: move |e| {
                            let s = e.value().clone();
                            log(("it moved lol: ", &s));
                            *selected_value.write() = s;
                            State::refresh();
                        },
                        option { value: "1", "24h" },
                        option { value: "2", "7d" },
                        option { value: "3", "30d" },
                        option { value: "4", "all" },
                    }
                }

                ul {
                    padding: "0",
                    margin: "0",
                    list_style_type: "none",
                    display: "flex",
                    flex_direction: "column",
                    max_height: "60vh",
                    overflow_y: "auto",
                    width: "250px",

                    li {
                        p {
                            margin_top: "40px",
                            ""
                        }
                    }

                    for task in tasks() {
                        li {
                            display: "flex",
                            flex_direction: "row",
                            margin_bottom: "10px",

                            div {
                                button {
                                    class: "emoji-button",
                                    font_size: "1.2em",
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
                                { tooltip(&task.priority, &format!("value: {}", &task.value), 0.8) }
                            }

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
