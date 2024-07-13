#![allow(non_snake_case)]

use super::*;

#[component]
pub fn About() -> Element {
    rsx! {
            div {
                display: "flex",
                justify_content: "center",
                align_items: "center",
                height: "100vh",
                flex_direction: "column",


            Link {to: Route::Home{}, "back"}

            p {
                "firelog, it's yet another task manager! but with a twist"
            }

            p {"basically, each task/habit has a value, you should use your own currency"}
            p {"recurring tasks get more important the longer since you did it (e.g. cleaning your room)"}
            p {"the 'value' basically means, if you were unable to do this task at a given moment, how much money would you pay to have it done?"}
            p {"since you also write in how long it takes to do the task, the value divided by the length (in hours) gives you the 'hourly wage' of each task"}
            p {"this means it'll ideally tell you which task has the best ROI at any given moment"}


        }
    }
}
