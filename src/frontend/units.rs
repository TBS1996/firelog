#![allow(non_snake_case)]

use super::*;

use crate::task::Tasks;
use crate::State;
use uuid::Uuid;

fn back_str() -> &'static str {
    include_str!("../../assets/return.svg")
}

#[component]
pub fn Units(id: Uuid) -> Element {
    let mut task = Tasks::load_offline().get_task(id).unwrap();
    let unit_name = task.unit_name();

    let mut input = Signal::new(String::new());

    let navigator = navigator();

    rsx! {
        div {
            display: "flex",
            flex_direction: "row",
            align_items: "center",
            justify_content: "center",

            button {
                class: "emoji-button",
                onclick: move |_| {
                    navigator.replace(Route::Home{});
                },
                height: "34px",
                margin_bottom: "20px",
                margin_left: "8px",
                img {
                    width: "20px",
                    height: "20px",
                    src: "{back_str()}",
                }
            }

            p {
                text_align: "center",
                "How many {unit_name} did you complete?"
            },
        }

        form {
            display: "flex",
            flex_direction: "row",
            align_items: "center",
            justify_content: "center",
            margin_top: "10px",
            onsubmit: move |event| {
                let data = event.data().values();
                let units: f32 = data.get("input").unwrap().as_value().to_string().parse().unwrap();
                task.do_task(units);
                navigator.replace(Route::Home {});
                State::refresh();
            },

            div {
                display: "flex",
                flex_direction: "row",
                align_items: "center",

                input {
                    r#type: "number",
                    value: input(),
                    name: "input",
                    autocomplete: "off",
                    oninput: move |event| input.set(event.value()),
                    width: "100px",
                    height: "34px",
                    text_align: "center",
                },
                button {
                    r#type: "submit",
                    class: "confirm",
                    height: "34px",
                    margin_left: "8px",
                    "submit"
                },
            }
        }
    }
}
