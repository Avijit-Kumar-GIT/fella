use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Input {
    Data { data: String },
    Resize { cols: u16, rows: u16 },
    Close,
}

enum Event {
    Input(Input),
    Output(String),
    ReaderClosed,
}

fn send_event(sender: &Sender<Event>, event: Event) -> bool {
    sender.send(event).is_ok()
}

fn send_output_bytes(sender: &Sender<Event>, pending: &mut Vec<u8>, bytes: &[u8]) -> bool {
    pending.extend_from_slice(bytes);
    let mut consumed = 0;
    loop {
        match std::str::from_utf8(&pending[consumed..]) {
            Ok(text) => {
                if !text.is_empty() && !send_event(sender, Event::Output(text.to_owned())) {
                    return false;
                }
                pending.clear();
                return true;
            }
            Err(error) => {
                let valid_length = error.valid_up_to();
                if valid_length > 0 {
                    let valid_end = consumed + valid_length;
                    let text = String::from_utf8_lossy(&pending[consumed..valid_end]).into_owned();
                    if !send_event(sender, Event::Output(text)) {
                        return false;
                    }
                    consumed = valid_end;
                }
                let Some(invalid_length) = error.error_len() else {
                    pending.drain(..consumed);
                    return true;
                };
                let invalid_end = consumed + invalid_length;
                let replacement =
                    String::from_utf8_lossy(&pending[consumed..invalid_end]).into_owned();
                if !send_event(sender, Event::Output(replacement)) {
                    return false;
                }
                consumed = invalid_end;
            }
        }
    }
}

fn flush_pending_output(sender: &Sender<Event>, pending: &mut Vec<u8>) -> bool {
    if pending.is_empty() {
        return true;
    }
    let output = String::from_utf8_lossy(pending).into_owned();
    pending.clear();
    send_event(sender, Event::Output(output))
}

fn emit(output: &mut impl Write, message: Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, &message)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn report_error(output: &mut impl Write, message: impl ToString) -> io::Result<()> {
    emit(
        output,
        json!({ "type": "error", "message": message.to_string() }),
    )
}

pub fn run(shell: PathBuf, cwd: PathBuf, cols: u16, rows: u16) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: rows.clamp(2, 500),
        cols: cols.clamp(2, 500),
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(error) => {
            report_error(
                &mut output,
                format!("Could not create a shell session: {error}"),
            )?;
            return Ok(());
        }
    };

    let mut command = CommandBuilder::new(&shell);
    command.cwd(&cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    let mut child = match pair.slave.spawn_command(command) {
        Ok(child) => child,
        Err(error) => {
            report_error(
                &mut output,
                format!("Could not start shell {}: {error}", shell.display()),
            )?;
            return Ok(());
        }
    };

    let mut reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            report_error(&mut output, format!("Could not read shell output: {error}"))?;
            return Ok(());
        }
    };
    let mut writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            let _ = child.kill();
            report_error(
                &mut output,
                format!("Could not send input to shell: {error}"),
            )?;
            return Ok(());
        }
    };

    emit(&mut output, json!({ "type": "started" }))?;

    let (sender, receiver) = mpsc::channel();
    let output_sender = sender.clone();
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        let mut pending = Vec::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => {
                    let _ = flush_pending_output(&output_sender, &mut pending);
                    let _ = send_event(&output_sender, Event::ReaderClosed);
                    break;
                }
                Ok(length) => {
                    if !send_output_bytes(&output_sender, &mut pending, &buffer[..length]) {
                        break;
                    }
                }
            }
        }
    });

    let input_sender = sender.clone();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in BufReader::new(stdin.lock()).lines() {
            let Ok(line) = line else { break };
            let Ok(input) = serde_json::from_str::<Input>(&line) else {
                continue;
            };
            let close = matches!(&input, Input::Close);
            if !send_event(&input_sender, Event::Input(input)) || close {
                return;
            }
        }
        let _ = send_event(&input_sender, Event::Input(Input::Close));
    });
    drop(sender);

    let mut reader_closed = false;
    loop {
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(Event::Output(data)) => emit(&mut output, json!({ "type": "data", "data": data }))?,
            Ok(Event::ReaderClosed) => reader_closed = true,
            Ok(Event::Input(Input::Data { data })) => {
                if writer.write_all(data.as_bytes()).is_err() || writer.flush().is_err() {
                    let _ = child.kill();
                    break;
                }
            }
            Ok(Event::Input(Input::Resize { cols, rows })) => {
                let _ = pair.master.resize(PtySize {
                    rows: rows.clamp(2, 500),
                    cols: cols.clamp(2, 500),
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            Ok(Event::Input(Input::Close)) => {
                let _ = child.kill();
                let status = child.wait()?;
                emit(
                    &mut output,
                    json!({
                        "type": "exit",
                        "exitCode": status.exit_code(),
                        "signal": status.signal(),
                    }),
                )?;
                return Ok(());
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        if let Some(status) = child.try_wait()? {
            emit(
                &mut output,
                json!({
                    "type": "exit",
                    "exitCode": status.exit_code(),
                    "signal": status.signal(),
                }),
            )?;
            return Ok(());
        }
        if reader_closed {
            let _ = child.kill();
        }
    }

    let status = child.wait()?;
    emit(
        &mut output,
        json!({
            "type": "exit",
            "exitCode": status.exit_code(),
            "signal": status.signal(),
        }),
    )
}

pub fn parse_args(
    args: impl Iterator<Item = OsString>,
) -> Result<(PathBuf, PathBuf, u16, u16), String> {
    let mut shell = None;
    let mut cwd = None;
    let mut cols = None;
    let mut rows = None;
    let mut args = args;
    while let Some(flag) = args.next() {
        let flag = flag
            .into_string()
            .map_err(|_| "Terminal argument names must be valid text".to_owned())?;
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--shell" => shell = Some(PathBuf::from(value)),
            "--cwd" => cwd = Some(PathBuf::from(value)),
            "--cols" => {
                cols = Some(
                    value
                        .to_string_lossy()
                        .parse::<u16>()
                        .map_err(|_| "--cols must be a number".to_owned())?,
                )
            }
            "--rows" => {
                rows = Some(
                    value
                        .to_string_lossy()
                        .parse::<u16>()
                        .map_err(|_| "--rows must be a number".to_owned())?,
                )
            }
            _ => return Err(format!("Unknown terminal argument: {flag}")),
        }
    }
    Ok((
        shell.ok_or_else(|| "--shell is required".to_owned())?,
        cwd.ok_or_else(|| "--cwd is required".to_owned())?,
        cols.ok_or_else(|| "--cols is required".to_owned())?,
        rows.ok_or_else(|| "--rows is required".to_owned())?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{send_output_bytes, Event};
    use std::sync::mpsc;

    #[test]
    fn output_decoder_keeps_utf8_sequences_split_between_pty_reads() {
        let (sender, receiver) = mpsc::channel();
        let mut pending = Vec::new();

        assert!(send_output_bytes(&sender, &mut pending, &[0xe2, 0x82]));
        assert!(receiver.try_recv().is_err());
        assert!(send_output_bytes(&sender, &mut pending, &[0xac]));

        assert!(matches!(receiver.try_recv(), Ok(Event::Output(text)) if text == "€"));
        assert!(pending.is_empty());
    }
}
