use crate::cache;
use dioxus::prelude::*;
use futures::executor::block_on;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::{window, Storage};

type UnixTime = Duration;

use crate::firebase::add_task_log_to_firestore;
use crate::sync::LogSyncRes;
use crate::{log, log_to_console, utils, State};

const DEFAULT_SLOPE: f32 = std::f32::consts::E + 1.;
pub type TaskID = Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub struct LogRecord {
    pub time: UnixTime,
    pub units: f32,
}

impl LogRecord {
    fn new(time: UnixTime, units: f32) -> Self {
        Self { time, units }
    }

    fn new_current(units: f32) -> Self {
        let time = utils::current_time();
        Self::new(time, units)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tasks(pub HashMap<Uuid, Task>);

impl Tasks {
    pub fn load_offline() -> Self {
        Self(block_on(cache::fetch_tasks()))
    }

    pub fn to_vec_sorted(self) -> Vec<Task> {
        let mut vec = vec![];

        for (_, task) in self.0.into_iter() {
            vec.push(task);
        }

        vec.sort_by_key(|t| (t.priority() * 1000.) as u32);
        vec.reverse();

        vec
    }

    pub fn prune_deleted(&mut self) {
        self.0.retain(|_, task| !task.metadata.deleted);
    }

    pub fn save_offline(&self) {
        log("starting save tasks");

        let mut metamap: HashMap<TaskID, MetaData> = HashMap::default();

        for (key, task) in &self.0 {
            metamap.insert(*key, task.metadata.clone());
        }

        Self::save_metadatas(metamap);
    }

    pub fn save_metadatas(metamap: HashMap<TaskID, MetaData>) {
        let s = serde_json::to_string(&metamap).unwrap();
        log(("save metadatas: ", &s));

        let storage: Storage = window()
            .expect("no global `window` exists")
            .local_storage()
            .expect("no local storage")
            .expect("local storage unavailable");

        storage
            .set_item("tasks", &s)
            .expect("Unable to set item in local storage");
        log_to_console("Stored tasks in local storage");
    }

    pub fn get_task(&self, id: Uuid) -> Option<Task> {
        self.0.get(&id).cloned()
    }

    pub fn insert(&mut self, task: Task) {
        self.0.insert(task.id, task);
    }

    pub fn delete_task(&mut self, id: Uuid) {
        let mut task = self.get_task(id).unwrap();
        task.metadata.deleted = true;
        task.metadata.updated = utils::current_time();
        self.insert(task);
        self.save_offline();
    }

    pub fn do_task(&mut self, id: Uuid, units: f32) {
        let mut task = self.get_task(id).unwrap();
        task.do_task(units);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaData {
    pub name: String,
    pub value: ValueEq,
    pub length: Duration,
    pub created: UnixTime,
    pub updated: UnixTime,
    pub deleted: bool,
}

impl MetaData {
    pub fn new(name: impl Into<String>, equation: ValueEq, length: Duration) -> Self {
        let time = utils::current_time();
        Self {
            name: name.into(),
            created: time,
            updated: time,
            value: equation,
            deleted: false,
            length,
        }
    }

    pub async fn save_offline(&self, id: TaskID) {
        log("starting save tasks");

        let mut metamap = cache::fetch_metadata().await;
        metamap.insert(id, self.clone());
        Tasks::save_metadatas(metamap);
    }

    pub fn from_jsvalue(val: wasm_bindgen::JsValue) -> HashMap<Uuid, Self> {
        let x: serde_json::Value = serde_wasm_bindgen::from_value(val).unwrap();
        log(("firetask: ", &x));
        let x = x.as_array().unwrap();

        let mut online_tasks = HashMap::default();

        for y in x {
            let task = y.get("task").unwrap().as_str().unwrap();
            let id = y.get("id").unwrap().as_str().unwrap();
            let task: MetaData = serde_json::from_str(&task).unwrap();
            let id: Uuid = serde_json::from_str(&id).unwrap();
            online_tasks.insert(id, task);
        }

        online_tasks
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskID,
    pub log: TaskLog,
    pub metadata: MetaData,
}

impl Task {
    pub fn new(name: impl Into<String>, equation: ValueEq, length: Duration) -> Self {
        Self {
            id: Uuid::new_v4(),
            metadata: MetaData::new(name, equation, length),
            log: TaskLog::default(),
        }
    }

    pub fn value(&self) -> f32 {
        self.metadata
            .value
            .value(&self.log, self.metadata.created, utils::current_time())
    }

    pub fn is_disc(&self) -> bool {
        match self.metadata.value {
            ValueEq::Log(_) => true,
            ValueEq::Cont(_) => false,
            ValueEq::Const(_) => false,
        }
    }

    pub fn set_unit_name(&mut self, s: String) {
        if let ValueEq::Cont(ref mut l) = &mut self.metadata.value {
            l.unit_name = Some(s);
            return;
        }

        panic!();
    }

    pub fn set_units(&mut self, units: f32) {
        if let ValueEq::Cont(ref mut l) = &mut self.metadata.value {
            l.daily_units = units;
            return;
        }

        panic!();
    }
    pub fn set_interval(&mut self, interval: Duration) {
        if let ValueEq::Log(ref mut l) = &mut self.metadata.value {
            l.interval = interval;
            return;
        }

        panic!();
    }

    pub fn set_factor(&mut self, factor: f32) {
        match &mut self.metadata.value {
            ValueEq::Log(x) => x.factor = factor,
            ValueEq::Cont(x) => x.factor = factor,
            ValueEq::Const(x) => *x = factor,
        };
    }

    pub fn factor(&self) -> f32 {
        match &self.metadata.value {
            ValueEq::Log(x) => x.factor,
            ValueEq::Cont(x) => x.factor,
            ValueEq::Const(x) => *x,
        }
    }

    pub fn _ratio(&self) -> f32 {
        if let ValueEq::Cont(l) = &self.metadata.value {
            return l.ratio(&self.log, utils::current_time());
        }

        panic!();
    }
    pub fn unit_name(&self) -> String {
        if let ValueEq::Cont(l) = &self.metadata.value {
            return l.unit_name.clone().unwrap_or("units".to_string());
        }

        panic!();
    }
    pub fn units(&self) -> f32 {
        if let ValueEq::Cont(l) = &self.metadata.value {
            return l.daily_units;
        }

        panic!();
    }

    pub fn interval(&self) -> Duration {
        if let ValueEq::Log(l) = &self.metadata.value {
            return l.interval;
        }

        panic!();
    }

    pub fn do_task(&mut self, units: f32) {
        let record = LogRecord::new_current(units);
        self.log.new(record);
        block_on(self.log.save_offline(self.id));

        let state = use_context::<State>();
        if let Some(user) = state.auth_user() {
            let future = add_task_log_to_firestore(user.uid, self.id, record);
            wasm_bindgen_futures::spawn_local(async {
                match future.await {
                    Ok(_) => web_sys::console::log_1(&JsValue::from_str("Log added successfully")),
                    Err(e) => web_sys::console::log_1(&e),
                }
            });
        }
    }

    /// Hourly wage
    pub fn priority(&self) -> f32 {
        let now = utils::current_time();
        let val = self
            .metadata
            .value
            .value(&self.log, self.metadata.created, now);

        let hour_length = self.metadata.length.as_secs_f32() / 3600.;
        val / hour_length
    }

    // Value accrued after 'dur'.
    pub fn value_since(&self, cutoff: UnixTime) -> f32 {
        let mut value_accrued = 0.;
        let tasklog = self.log.clone();

        let mut inner = vec![];
        for log in &tasklog.0 {
            let time = log.time;
            if time > cutoff {
                let value = self.metadata.value.value(
                    &TaskLog::newlol(inner.clone()),
                    self.metadata.created,
                    time,
                );

                value_accrued += value;
            }
            inner.push(log.clone());
        }

        value_accrued
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskLog(Vec<LogRecord>);

impl TaskLog {
    fn new(&mut self, record: LogRecord) {
        if !self.0.contains(&record) {
            self.0.push(record);
        }
    }

    pub fn time_since(&self, time: UnixTime) -> Vec<Duration> {
        let mut vec = vec![];

        for log in &self.0 {
            vec.push(time - log.time);
        }

        vec
    }

    pub fn last_completed(&self) -> Option<UnixTime> {
        self.0.last().copied().map(|rec| rec.time)
    }

    fn newlol(mut logs: Vec<LogRecord>) -> Self {
        logs.sort_by_key(|log| log.time);
        Self(logs)
    }

    pub async fn sync_id(id: TaskID, uid: String) -> LogSyncRes {
        use crate::firebase;

        let offline_logs = TaskLog::load_logs(id).await;
        let online_logs = {
            let val = firebase::load_logs_for_task(uid.clone(), id).await.unwrap();

            Self::from_jsvalue(val)
        };

        let mut res = TaskLog::sync(online_logs, offline_logs);
        res.id = id;
        res.user_id = uid;
        res
    }

    pub fn sync(from_online: Self, from_offline: Self) -> LogSyncRes {
        let mut res = LogSyncRes::default();
        let mut send_up = vec![];
        let mut save = vec![];

        for unix in from_offline.0 {
            if !from_online.0.contains(&unix) {
                send_up.push(unix);
            }

            if !save.contains(&unix) {
                save.push(unix);
            }
        }

        for unix in from_online.0 {
            if !save.contains(&unix) {
                save.push(unix);
            }
        }

        send_up.sort_by_key(|rec| rec.time);

        res.send_up = send_up;
        res.save = Self::newlol(save);

        res
    }

    pub fn from_jsvalue(val: JsValue) -> Self {
        let mut logs = vec![];
        let val: serde_json::Value = serde_wasm_bindgen::from_value(val).unwrap();
        let arr = val.as_array().unwrap().clone();

        for el in arr {
            let ts: u64 = el
                .as_object()
                .unwrap()
                .get("timestamp")
                .unwrap()
                .as_str()
                .unwrap()
                .parse()
                .unwrap();

            let ts = UnixTime::from_secs(ts);

            let units: f32 = match el.as_object().unwrap().get("units").unwrap().as_str() {
                Some(s) => s.parse().unwrap(),
                None => 1.,
            };

            let log = LogRecord::new(ts, units);
            logs.push(log);
        }

        logs.sort_by_key(|log| log.time);
        Self(logs)
    }

    fn merge(&mut self, other: Self) {
        let mut merged = vec![];

        for log in &self.0 {
            if !merged.contains(log) {
                merged.push(*log);
            }
        }

        for log in &other.0 {
            if !merged.contains(log) {
                merged.push(*log);
            }
        }

        merged.sort_by_key(|mrg| mrg.time);

        *self = Self(merged);
    }

    pub async fn load_logs(task: TaskID) -> Self {
        cache::fetch_logs()
            .await
            .get(&task)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn save_offline(&self, id: TaskID) {
        let mut all_logs = cache::fetch_logs().await;
        let mut current = all_logs.get(&id).cloned().unwrap_or_default();
        current.merge(self.clone());
        all_logs.insert(id, current);

        log("starting save logs");
        cache::save_logs(all_logs);
        log_to_console("Stored logs in local storage");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogPriority {
    interval: UnixTime,
    factor: f32,
    slope: f32,
}

impl LogPriority {
    pub fn new(factor: f32, interval: Duration) -> Self {
        Self {
            interval,
            factor,
            slope: DEFAULT_SLOPE,
        }
    }

    fn value(&self, t: Duration) -> f32 {
        let ratio = t.as_secs_f32() / self.interval.as_secs_f32();
        self.factor * val_calc::value(ratio, self.slope)
    }
}

pub mod val_calc {
    pub fn value(ratio: f32, slope: f32) -> f32 {
        ((slope - 2.) * ratio + 1.).log(slope - 1.)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValueEq {
    Log(LogPriority),
    Const(f32),
    Cont(Contask),
}

impl ValueEq {
    pub fn value(&self, logs: &TaskLog, created: UnixTime, current_time: UnixTime) -> f32 {
        match self {
            Self::Const(f) => *f,
            Self::Cont(c) => c.value(logs, current_time),
            Self::Log(log) => {
                let last_completed = logs.last_completed().unwrap_or(created - log.interval);
                let time_since = current_time - last_completed;
                log.value(time_since)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contask {
    // How many units you're expected to do per day on avg
    daily_units: f32,
    //  when you do one unit at the average rate, how much is the value?
    factor: f32,

    created: UnixTime,

    unit_name: Option<String>,
}

impl Contask {
    pub fn new(daily_units: f32, factor: f32, unit_name: String) -> Self {
        let created = utils::current_time();

        Self {
            daily_units,
            factor,
            created,
            unit_name: Some(unit_name),
        }
    }

    fn value(&self, logs: &TaskLog, current: UnixTime) -> f32 {
        let ratio = self.ratio(logs, current);
        self.factor * val_calc::value(ratio, DEFAULT_SLOPE)
    }

    fn ratio(&self, logs: &TaskLog, current: UnixTime) -> f32 {
        let avg = self.daily_average(logs, current);
        log(("avg: ", avg));
        self.daily_units / avg
    }

    fn daily_average(&self, logs: &TaskLog, current: UnixTime) -> f32 {
        let days_elapsed = (current - self.created).as_secs_f32() / 86400.;
        let total_units: f32 = logs.0.iter().map(|log| log.units).sum();

        // We add the daily_units so that when you create the task for the first time the avg isnt
        // 0 and thus the value infinite.
        (total_units + self.daily_units) / (days_elapsed + 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log() {
        let day = Duration::from_secs(86400);
        let x = LogPriority::new(1.0, day);
        let y = x.value(day);
        assert_eq!(y, 1.0);
    }
}
