#![allow(non_snake_case)]

use super::*;

use crate::task::Tasks;
use crate::State;
use uuid::Uuid;

#[component]
pub fn Units(id: Uuid) -> Element {
    let mut task = Tasks::load_offline().get_task(id).unwrap();
    let unit_name = task.unit_name();

    let mut input = Signal::new(String::new());

    let navigator = navigator();

    rsx! {

        div {
            display: "flex",
            justify_content: "center",
            align_items: "center",
            height: "100vh",


            div {
                padding: "20px",

            Link { to: Route::Home {}, "back" }



        form {
            display: "flex",
            flex_direction: "row",
            onsubmit: move |event| {

                let data = event.data().values();
                let units: f32 = data.get("input").unwrap().as_value().to_string().parse().unwrap();
                task.do_task(units);
                navigator.replace(Route::Home {});
                State::refresh();


            },
            div {
                class: "input-group",
                display: "flex",
                flex_direction: "column",

                div {
                    display: "flex",
                    flex_direction: "row",
                    justify_content: "space-between",
                    "{unit_name}"
                    input {
                        r#type: "text",
                        value: input(),
                        name: "input",
                        autocomplete: "off",
                        oninput: move |event| input.set(event.value()),
                    }
                }

                button {
                    r#type: "submit",
                    class: "confirm",
                    "submit"
                }
           }
        }
            }
        }

    }
}
