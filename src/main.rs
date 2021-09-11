// SPDX-License-Identifier: GPL-2.0-only
/*
 * Copyright (C) 2021 Carles Pey <cpey@pm.me>
 */

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, FixedOffset, Local};
use std::fs::File;
use std::io::{self, prelude::*};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

const SYSLOG_FNAME: &str = "syslog.log";
const STDIN_FNAME: &str = "stdin.log";
const FUSED_FNAME: &str = "result.log";
const UNIXTIME: &str = "1970-01-01T00:00:00.000000+00:00";

fn ctrl_channel() -> Result<mpsc::Receiver<()>> {
    let (sender, receiver) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = sender.send(());
    })?;

    Ok(receiver)
}

fn collect_syslog() -> Result<()> {
    let ctrl_events = ctrl_channel()?;
    let f_sys = File::create(SYSLOG_FNAME)?;

    let mut logger = Command::new("dmesg")
        .arg("--time-format")
        .arg("iso")
        .arg("-w")
        .stdout(Stdio::from(f_sys))
        .spawn()
        .expect("logger failed to start");

    ctrl_events.recv().unwrap();
    logger.kill()?;
    Ok(())
}

fn collect_stdin() -> Result<()> {
    let mut f_in = File::create(STDIN_FNAME)?;

    let thread = thread::spawn(move || -> Result<()> {
        let dt = Local::now();
        let stdin = io::stdin();
        let mut _str;

        for line_result in stdin.lock().lines() {
            let line = line_result?;
            _str = format!("{} {}\n", dt.format("%+"), line);
            write!(f_in, "{}", &_str)?;
        }
        Ok(())
    });

    thread.join().unwrap()?;
    Ok(())
}

fn collect_logs() -> Result<()> {
    collect_syslog()?;
    collect_stdin()
}

fn read_lines(filename: &str) -> Result<io::Lines<io::BufReader<File>>, anyhow::Error> {
    let file = File::open(filename).with_context(|| format!("Failed to read {}", filename))?;
    Ok(io::BufReader::new(file).lines())
}

fn get_line(line: &Option<Result<String, std::io::Error>>) -> Result<String, anyhow::Error> {
    let mut _str: String;

    match line {
        Some(_line) => match _line {
            Ok(v) => {
                _str = v.to_string() + "\n";
                return Ok(_str);
            }
            Err(_) => return Err(anyhow!("Error processing line")),
        },
        None => Err(anyhow!("Error processing line")),
    }
}

fn get_date(
    line: &Option<Result<String, std::io::Error>>,
) -> Result<DateTime<FixedOffset>, anyhow::Error> {
    let mut _str: String;
    let date: String;

    match line {
        Some(_line) => match _line {
            Ok(v) => {
                _str = v.to_string();
                date = _str.split_ascii_whitespace().next().unwrap().to_string();
            }
            Err(_) => return Err(anyhow!("Error processing line")),
        },
        None => {
            return Err(anyhow!("Empty line"));
        }
    }

    // chrono fails parsing ISO-8601 when there is a comma for the decimal separator
    match DateTime::parse_from_rfc3339(&date.replace(",", ".")) {
        Ok(v) => Ok(v),
        Err(err) => Err(anyhow!(err)),
    }
}

fn fuse_logs() -> Result<()> {
    let mut _line_stdin: Option<Result<String, std::io::Error>> = None;
    let mut _line_syslog: Option<Result<String, std::io::Error>> = None;

    let mut stdin_date = DateTime::parse_from_rfc3339(&UNIXTIME).unwrap();
    let mut syslog_date = DateTime::parse_from_rfc3339(&UNIXTIME).unwrap();

    let mut syslog_lines = read_lines(SYSLOG_FNAME)?;
    let mut stdin_lines = read_lines(STDIN_FNAME)?;
    let mut f_out = File::create(FUSED_FNAME)?;

    let mut exhausted_stdin = false;
    let mut exhausted_syslog = false;
    let mut used_stdin = true;
    let mut used_syslog = true;

    loop {
        if used_stdin {
            _line_stdin = stdin_lines.next();
            match get_date(&_line_stdin) {
                Ok(v) => {
                    stdin_date = v;
                    used_stdin = false;
                }
                Err(_) => {
                    exhausted_stdin = true;
                    break;
                }
            }
        }

        if used_syslog {
            _line_syslog = syslog_lines.next();
            match get_date(&_line_syslog) {
                Ok(v) => {
                    syslog_date = v;
                    used_syslog = false;
                }
                Err(_) => {
                    exhausted_syslog = true;
                    break;
                }
            }
        }

        if syslog_date < stdin_date {
            write!(f_out, "{}", get_line(&_line_syslog)?)?;
            used_syslog = true;
        } else {
            write!(f_out, "{}", get_line(&_line_stdin)?)?;
            used_stdin = true;
        }
    }

    if exhausted_stdin {
        loop {
            if let Ok(_line) = get_line(&_line_syslog) {
                write!(f_out, "{}", _line)?;
            } else {
                break;
            }
            _line_syslog = syslog_lines.next();
        }
    } else if exhausted_syslog {
        loop {
            if let Ok(_line) = get_line(&_line_stdin) {
                write!(f_out, "{}", _line)?;
            } else {
                break;
            }
            _line_stdin = stdin_lines.next();
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    collect_logs()?;
    fuse_logs()?;
    Ok(())
}
