use crate::cache;
use crate::firebase;
use crate::frontend::{task_props, tot_value_since};
use crate::task::{LogRecord, MetaData, Task, TaskLog, Tasks};
use crate::{log, State};
use dioxus::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

#[derive(Default)]
struct SyncResult {
    send_up: Vec<Task>,
    download: HashMap<Uuid, MetaData>,
}

#[derive(Default, Debug)]
struct Syncer {
    pairs: Vec<(Task, MetaData)>,
    new_from_server: HashMap<Uuid, MetaData>,
    new_offline: Vec<Task>,
}

impl Syncer {
    fn new(mut online: HashMap<Uuid, MetaData>, offline: Tasks) -> Self {
        let mut selv = Self::default();
        for (_, off_task) in offline.0 {
            match online.remove(&off_task.id) {
                Some(ontask) => {
                    selv.pairs.push((off_task, ontask));
                }
                None => {
                    selv.new_offline.push(off_task);
                }
            };
        }

        selv.new_from_server = online;

        selv
    }

    fn sync(self) -> SyncResult {
        let mut res = SyncResult::default();
        for (off, on) in self.pairs {
            if off.metadata.updated > on.updated {
                res.send_up.push(off);
            } else if off.metadata.updated < on.updated {
                res.download.insert(off.id, on);
            }
        }

        for task in self.new_from_server {
            res.download.insert(task.0, task.1);
        }

        for task in self.new_offline {
            res.send_up.push(task);
        }

        res
    }
}

#[derive(Default)]
pub struct LogSyncRes {
    pub send_up: Vec<LogRecord>,
    pub save: TaskLog,
    pub id: Uuid,
    pub user_id: String,
}

pub fn sync_tasks(mut is_syncing: Signal<bool>) {
    let state = use_context::<State>();

    let x = (*state.inner.lock().unwrap().auth_status.read()).clone();

    let mut tasks = state.inner.lock().unwrap().tasks.clone();
    let mut value_stuff = state.inner.lock().unwrap().value_stuff.clone();

    let Some(user) = x.user() else {
        tasks.set(task_props());
        value_stuff.set(tot_value_since());
        return;
    };

    let task_future =
        wasm_bindgen_futures::JsFuture::from(firebase::loadAllTasks(&JsValue::from_str(&user.uid)));
    let offline_tasks = Tasks::load_offline();

    wasm_bindgen_futures::spawn_local(async move {
        is_syncing.set(true);
        let online_tasks = MetaData::from_jsvalue(task_future.await.unwrap());

        let res = Syncer::new(online_tasks, offline_tasks).sync();

        let futs: Vec<_> = res
            .send_up
            .iter()
            .map(|task| firebase::send_task_to_firestore(user.uid.clone(), &task))
            .collect();

        futures::future::join_all(futs).await;

        for (id, task) in res.download {
            let metadata = MetaData {
                name: task.name,
                value: task.value,
                length: task.length,
                created: task.created,
                updated: task.updated,
                deleted: task.deleted,
            };

            metadata.save_offline(id).await;
        }

        log("syncing logs");
        let all_tasks = cache::fetch_tasks().await;

        let futs: Vec<_> = all_tasks
            .into_keys()
            .into_iter()
            .map(|key| TaskLog::sync_id(key, user.uid.clone()))
            .collect();

        let vals = futures::future::join_all(futs).await;

        let mut outer_futs = vec![];

        for res in vals {
            res.save.save_offline(res.id).await;
            let futs: Vec<_> = res
                .send_up
                .iter()
                .map(|x| firebase::add_task_log_to_firestore(user.uid.clone(), res.id, *x))
                .collect();

            outer_futs.push(futures::future::join_all(futs));
        }

        futures::future::join_all(outer_futs).await;

        is_syncing.set(false);
        tasks.set(task_props());
        value_stuff.set(tot_value_since());
    });
}
