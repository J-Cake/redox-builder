use std::sync::Weak;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ReportMode {
    Auto,
    Tui,
    Text,
    Json
}

pub struct StatusUpdate {

}

struct Status {
    parent: Option<Weak<Status>>,
    status: StatusLevel,

}

enum StatusLevel {
    Pending(String),
    Success,
    Error(String)
}

#[derive(Debug)]
pub struct Reporter {
    report_mode: ReportMode,
    sender: crossbeam::channel::Sender<StatusUpdate>,
    receiver: crossbeam::channel::Receiver<StatusUpdate>,
}

impl Reporter {
    pub fn new(mode: ReportMode) -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();

        Self {
            report_mode: mode,
            sender,
            receiver
        }
    }
}