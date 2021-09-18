// SPDX-License-Identifier: GPL-2.0-only
/*
 * Copyright (C) 2021 Carles Pey <cpey@pm.me>
 */

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, FixedOffset, Local};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::fs::{self, File};
use std::io::{self, prelude::*, Write};
use std::process::{ChildStdout, Command, Stdio};
use std::str;
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use structopt::StructOpt;

const SYSLOG_FNAME: &str = "/tmp/dmerg.syslog";
const STDIN_FNAME: &str = "/tmp/dmerg.stdin";
const FUSED_FNAME: &str = "dmerged";
const UNIXTIME: &str = "1970-01-01T00:00:00.000000+00:00";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.6f%z";

fn ctrl_channel() -> Result<(Receiver<()>, Receiver<()>)> {
    let (sender1, receiver1) = channel();
    let (sender2, receiver2) = channel();

    ctrlc::set_handler(move || {
        let _ = sender1.send(());
        let _ = sender2.send(());
    })?;

    Ok((receiver1, receiver2))
}

fn get_logfile(name: &str, rand: &str) -> String {
    let filename: String = format!("{}.{}", name, rand);
    return filename;
}

fn get_syslog_line(reader: io::BufReader<ChildStdout>) -> Result<Receiver<String>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if let Err(_) = tx.send(l) {
                        continue;
                    }
                }
                Err(_) => {
                    continue;
                }
            }
        }
    });

    Ok(rx)
}

fn collect_syslog(
    rand: &str,
    args: &Opt,
    recv: Receiver<()>,
) -> Result<thread::JoinHandle<Result<()>>> {
    let mut f_sys = File::create(get_logfile(SYSLOG_FNAME, rand))?;
    let ctime_iso = Local::now().format("%+").to_string();
    let ctime_dt = DateTime::parse_from_rfc3339(&ctime_iso).unwrap();
    let full = args.full;
    let console_off = args.console_off;
    let dmesg = args.dmesg;

    let thread = thread::spawn(move || -> Result<()> {
        let mut logger = if dmesg {
            Command::new("dmesg")
                .arg("--time-format")
                .arg("iso")
                .arg("-w")
                .stdout(Stdio::piped())
                .spawn()
                .expect("Failed to execute dmesg")
        } else {
            // Check we have journal access permissions
            let test = Command::new("journalctl")
                .arg("-k")
                .output()
                .expect("Failed to execute journalctl");
            if !test.status.success() {
                return Err(anyhow!(match str::from_utf8(&test.stderr) {
                    Ok(v) => v.to_string(),
                    Err(e) => format!("Err: {}", e),
                }));
            }
            Command::new("journalctl")
                .arg("-k")
                .arg("-f")
                .arg("-o")
                .arg("short-iso-precise")
                .stdout(Stdio::piped())
                .spawn()
                .expect("Failed to execute journalctl")
        };

        let reader = io::BufReader::new(logger.stdout.take().expect("Failed to capture stdout"));
        let rx = match get_syslog_line(reader) {
            Ok(r) => r,
            Err(_) => return Err(anyhow!("Error generating communication channels")),
        };

        loop {
            match rx.try_recv() {
                Ok(line) => {
                    if let Ok(date) = get_line_split(&Some(Ok(line.clone()))) {
                        if full || date.0 >= ctime_dt {
                            let date_iso = date.0.format(TIME_FORMAT).to_string();
                            writeln!(f_sys, "{} {}", &date_iso, date.1)?;
                            if !console_off {
                                println!("{} {}", &date_iso, date.1);
                                io::stdout().flush().unwrap();
                            }
                        }
                    }
                }
                Err(_) => {}
            }
            match recv.try_recv() {
                Ok(()) => {
                    logger.kill()?;
                    break;
                }
                Err(_) => {}
            }
        }

        Ok(())
    });

    return Ok(thread);
}

fn get_stdin_line() -> Result<Receiver<String>> {
    let (tx, rx) = channel();
    let stdin = io::stdin();
    thread::spawn(move || {
        for line_result in stdin.lock().lines() {
            match line_result {
                Ok(l) => {
                    if let Err(_) = tx.send(l) {
                        continue;
                    }
                }
                Err(_) => {
                    continue;
                }
            }
        }
    });

    Ok(rx)
}

fn collect_stdin(
    rand: &str,
    args: &Opt,
    recv: Receiver<()>,
) -> Result<thread::JoinHandle<Result<()>>> {
    let mut f_in = File::create(get_logfile(STDIN_FNAME, rand))?;
    let console_off = args.console_off;
    let thread = thread::spawn(move || -> Result<()> {
        let rx = match get_stdin_line() {
            Ok(r) => r,
            Err(_) => return Err(anyhow!("Error generating communication channels")),
        };

        loop {
            match rx.try_recv() {
                Ok(line) => {
                    let dt = Local::now();
                    let _str = format!("{} {}\n", dt.format(TIME_FORMAT).to_string(), line);
                    write!(f_in, "{}", &_str)?;
                    if !console_off {
                        print!("{}", &_str);
                        io::stdout().flush().unwrap();
                    }
                }
                Err(_) => {}
            }
            match recv.try_recv() {
                Ok(()) => break,
                Err(_) => {}
            }
        }
        Ok(())
    });

    return Ok(thread);
}

fn collect_logs(rand: &str, args: &Opt) -> Result<()> {
    let receivers;
    let th_stdin;
    let th_syslog;

    match ctrl_channel() {
        Ok(r) => receivers = r,
        Err(_) => return Err(anyhow!("Error generating communication channels")),
    };

    match collect_syslog(&rand, &args, receivers.0) {
        Ok(t) => th_syslog = t,
        Err(_) => return Err(anyhow!("Error generating thread")),
    }

    match collect_stdin(&rand, &args, receivers.1) {
        Ok(t) => th_stdin = t,
        Err(_) => return Err(anyhow!("Error generating thread")),
    }

    th_syslog.join().unwrap()?;
    th_stdin.join().unwrap()?;

    return Ok(());
}

fn read_lines(filename: &str) -> Result<io::Lines<io::BufReader<File>>> {
    let file = File::open(filename).with_context(|| format!("Failed to read {}", filename))?;
    Ok(io::BufReader::new(file).lines())
}

fn get_line(line: &Option<Result<String, std::io::Error>>) -> Result<String> {
    match line {
        Some(_line) => match _line {
            Ok(v) => Ok(v.to_string()),
            Err(e) => Err(anyhow!("Error processing line: {}", e)),
        },
        None => Err(anyhow!("Error processing line")),
    }
}

fn get_line_split(
    line: &Option<Result<String, std::io::Error>>,
) -> Result<(DateTime<FixedOffset>, String)> {
    let date: String;
    let msg: String;

    match line {
        Some(_line) => match _line {
            Ok(v) => {
                let _str = v.to_string();
                let mut split = _str.split_whitespace();
                date = split.next().unwrap().to_string();
                let split_end: Vec<_> = split.collect();
                msg = split_end.join(" ");
            }
            Err(_) => return Err(anyhow!("Error processing line")),
        },
        None => {
            return Err(anyhow!("Empty line"));
        }
    }

    // journald time format: 2021-09-17T07:24:29.446013+0000
    // dmesg time format: 2021-09-17T07:24:23,364133+00:00
    // chrono fails parsing ISO-8601 when there is a comma for the decimal separator
    match DateTime::parse_from_str(&date.replace(",", "."), TIME_FORMAT) {
        Ok(v) => Ok((v, msg)),
        Err(err) => Err(anyhow!(err)),
    }
}

fn dump_bufreader(
    f_out: &mut File,
    mut reader_lines: io::Lines<io::BufReader<File>>,
    mut curr_line: Option<Result<String, std::io::Error>>,
) -> Result<()> {
    loop {
        if let Ok(_line) = get_line(&curr_line) {
            writeln!(f_out, "{}", _line)?;
        } else {
            break;
        }
        curr_line = reader_lines.next();
    }
    return Ok(());
}

fn merge_logs(rand: &str, args: &Opt) -> Result<()> {
    let output_file: String;
    match &args.output {
        Some(v) => output_file = v.to_string(),
        None => output_file = get_logfile(FUSED_FNAME, rand),
    }

    let mut _line_stdin: Option<Result<String, std::io::Error>> = None;
    let mut _line_syslog: Option<Result<String, std::io::Error>> = None;

    let mut stdin_date = DateTime::parse_from_rfc3339(&UNIXTIME).unwrap();
    let mut syslog_date = DateTime::parse_from_rfc3339(&UNIXTIME).unwrap();

    let mut syslog_lines = read_lines(&get_logfile(SYSLOG_FNAME, rand))?;
    let mut stdin_lines = read_lines(&get_logfile(STDIN_FNAME, rand))?;
    let mut f_out = File::create(output_file)?;

    let mut end_stdin = false;
    let mut end_syslog = false;
    let mut next_stdin = true;
    let mut next_syslog = true;

    loop {
        if next_stdin {
            _line_stdin = stdin_lines.next();
            match get_line_split(&_line_stdin) {
                Ok(v) => {
                    stdin_date = v.0;
                    next_stdin = false;
                }
                Err(_) => {
                    end_stdin = true;
                }
            }
        }

        if next_syslog {
            _line_syslog = syslog_lines.next();
            match get_line_split(&_line_syslog) {
                Ok(v) => {
                    syslog_date = v.0;
                    next_syslog = false;
                }
                Err(_) => {
                    end_syslog = true;
                }
            }
        }

        // We exit the loop here to be sure each file is read at least once
        if end_stdin || end_syslog {
            break;
        }

        if syslog_date < stdin_date {
            writeln!(f_out, "{}", get_line(&_line_syslog)?)?;
            next_syslog = true;
        } else {
            writeln!(f_out, "{}", get_line(&_line_stdin)?)?;
            next_stdin = true;
        }
    }

    if end_stdin {
        dump_bufreader(&mut f_out, syslog_lines, _line_syslog)?;
    } else if end_syslog {
        dump_bufreader(&mut f_out, stdin_lines, _line_stdin)?;
    }

    Ok(())
}

fn remove_tmp_files(rand: &str) -> Result<()> {
    fs::remove_file(get_logfile(SYSLOG_FNAME, rand))?;
    fs::remove_file(get_logfile(STDIN_FNAME, rand))?;
    Ok(())
}

fn notify_result(rand: &str, args: &Opt) -> Result<()> {
    let mut rand_file = false;
    let output_file = match &args.output {
        Some(v) => v.to_string(),
        None => {
            rand_file = true;
            get_logfile(FUSED_FNAME, rand)
        }
    };

    if !args.console_off || rand_file {
        eprintln!("\n+ Output written to {}", output_file);
    }
    Ok(())
}

#[derive(StructOpt)]
struct Opt {
    /// Include full kernel log output.
    #[structopt(short, long)]
    full: bool,
    /// Write output to <output> instead of a random file.
    #[structopt(short, long)]
    output: Option<String>,
    /// Do not write to the standard output.
    #[structopt(short, long)]
    console_off: bool,
    /// Use dmesg instead of journald
    #[structopt(short, long)]
    dmesg: bool,
}

fn main() -> Result<()> {
    let args = Opt::from_args();
    let rand: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();

    if let Err(e) = collect_logs(&rand, &args) {
        remove_tmp_files(&rand)?;
        return Err(anyhow!(e));
    };

    if let Err(e) = merge_logs(&rand, &args) {
        remove_tmp_files(&rand)?;
        return Err(anyhow!(e));
    }

    remove_tmp_files(&rand)?;
    notify_result(&rand, &args)?;
    Ok(())
}
