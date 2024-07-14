#![allow(non_snake_case)]

use super::*;

use crate::task::{Task, Tasks};
use crate::utils;
use crate::State;
use uuid::Uuid;

#[component]
pub fn Editcont(id: Uuid) -> Element {
    let task = Tasks::load_offline().get_task(id).unwrap();
    let thetask = Tasks::load_offline().get_task(id).unwrap();

    let navigator = use_navigator();

    let closure = move |newtask: Option<Task>| {
        let Some(newtask) = newtask else {
            return;
        };
        let mut oldtask = task.clone();

        log("submitting!");

        log("success!");
        oldtask.set_factor(newtask.factor());
        oldtask.set_units(newtask.units());
        oldtask.set_unit_name(newtask.unit_name());
        oldtask.metadata.name = newtask.metadata.name;
        oldtask.metadata.length = newtask.metadata.length;
        oldtask.metadata.updated = utils::current_time();

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


                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    width: "200px",

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
                }

            { form }

            }
        }
    }
}

#[component]
pub fn Edit(id: Uuid) -> Element {
    let task = Tasks::load_offline().get_task(id).unwrap();

    let oldtask = task.clone();
    log(&oldtask);
    let navigator = use_navigator();

    let logstr: Vec<String> = task
        .log
        .time_since(utils::current_time())
        .into_iter()
        .map(|dur| utils::dur_format(dur))
        .collect();
    let logstr = format!("{:?}", logstr);
    let mut logstr = logstr.replace("\"", "");
    logstr.pop();
    logstr.remove(0);

    let closure = move |newtask: Option<Task>| {
        let mut oldtask = task.clone();
        let Some(newtask) = newtask else {
            return;
        };

        log(("success! new task: ", &newtask));
        oldtask.set_factor(newtask.factor());
        oldtask.set_interval(newtask.interval());
        oldtask.metadata.name = newtask.metadata.name;
        oldtask.metadata.length = newtask.metadata.length;
        oldtask.metadata.updated = utils::current_time();
        log(("edited task: ", &oldtask));

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
                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    width: "200px",

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
                        margin_left: "20px",
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
                }

            { form }

            }
        }
    }
}
