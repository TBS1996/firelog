#![allow(non_snake_case)]

use super::*;

use crate::task::Tasks;
use crate::utils;
use uuid::Uuid;

#[component]
pub fn Stats(id: Uuid) -> Element {
    let task = Tasks::load_offline().get_task(id).unwrap();
    let mut stats: Vec<(String, String)> = vec![];

    stats.push(("meta".to_string(), format!("{:?}", &task.metadata)));
    stats.push(("log".to_string(), utils::logstr(&task.log)));
    if !task.is_disc() {
        stats.push(("daily-avg".to_string(), format!("{:?}", &task.daily_avg())));
    }

    rsx! {
        div {
            display: "flex",
            flex_direction: "column",
            for (key, val) in stats {
                div {
                    display: "flex",
                    flex_direction: "row",
                    p {"{key}"}
                    p {margin_left: "40px","{val}"}
                }
            }
        }
    }
}
