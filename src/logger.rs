use chrono::prelude::*;
use std::ffi::OsString;

#[derive(Clone, Debug)]
pub enum Level {
    Debug,
    Warn,
    Quiet,
}

impl Level {
    pub fn from_flag(flag: &str) -> Result<Level, String> {
        match flag {
            "-d" => Ok(Level::Debug),
            "-w" => Ok(Level::Warn),
            "-q" => Ok(Level::Quiet),
            other => Err(format!(
                "unknown log level '{}'; expected -d, -w, or -q",
                other
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Logger {
    pub host: OsString,
    pub level: Level,
}

impl Logger {
    pub fn new(host: OsString, level: Level) -> Self {
        Logger { host, level }
    }

    pub fn info(&self, msg: &str) {
        println!(
            "[{} INFO ] - linux disk space manager - {:?} - {}",
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            self.host,
            msg
        );
    }

    pub fn warn(&self, msg: &str) {
        match self.level {
            Level::Debug | Level::Warn => println!(
                "[{} WARN ] - linux disk space manager - {:?} - {}",
                Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
                self.host,
                msg
            ),
            Level::Quiet => {}
        }
    }

    pub fn debug(&self, msg: &str) {
        if let Level::Debug = self.level {
            println!(
                "[{} DEBUG] - linux disk space manager - {:?} - {}",
                Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
                self.host,
                msg
            );
        }
    }

    pub fn error(&self, msg: &str) {
        println!(
            "[{} ERROR] - linux disk space manager - {:?} - {}",
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            self.host,
            msg
        );
    }
}
