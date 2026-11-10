//! Embedded SSH text console running on the workspace Tokio runtime.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::{ChannelMsg, Disconnect, client};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::worker::Event;

#[derive(Clone, Debug)]
pub enum SshInput {
    Data(Vec<u8>),
    Close,
}

#[derive(Default)]
struct SshRegistry {
    channels: HashMap<String, UnboundedSender<SshInput>>,
    cancelled: HashSet<String>,
}

#[derive(Clone, Default)]
pub struct SshHandles(Arc<Mutex<SshRegistry>>);

impl SshHandles {
    fn register(&self, console_id: &str, sender: UnboundedSender<SshInput>) -> bool {
        let Ok(mut registry) = self.0.lock() else {
            return false;
        };
        if registry.cancelled.remove(console_id) {
            return false;
        }
        registry.channels.insert(console_id.to_owned(), sender);
        true
    }

    fn remove(&self, console_id: &str) {
        if let Ok(mut registry) = self.0.lock() {
            registry.channels.remove(console_id);
        }
    }

    fn send(&self, console_id: &str, input: SshInput) {
        let Ok(mut registry) = self.0.lock() else {
            return;
        };
        if let Some(sender) = registry.channels.get(console_id) {
            if sender.send(input).is_err() {
                registry.channels.remove(console_id);
            }
            return;
        }
        if matches!(input, SshInput::Close) {
            registry.cancelled.insert(console_id.to_owned());
        }
    }
}

pub fn send_input(handles: &SshHandles, console_id: &str, input: SshInput) {
    handles.send(console_id, input);
}

pub fn close(handles: &SshHandles, console_id: &str) {
    handles.send(console_id, SshInput::Close);
}

struct Client;

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Guest appliance host keys are ephemeral and usually not known in
        // advance. The console is explicitly opened by the operator; accept it.
        Ok(true)
    }
}

pub fn validate_credentials(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let host = host.trim();
    if host.is_empty() || host.chars().any(char::is_whitespace) {
        return Err("Enter the guest host name or IP address.".into());
    }
    if host.chars().count() > 255 {
        return Err("The SSH host is too long.".into());
    }
    if port == 0 {
        return Err("SSH port must be between 1 and 65535.".into());
    }
    if username.trim().is_empty() || username.chars().count() > 255 {
        return Err("Enter the SSH user name.".into());
    }
    if password.is_empty() || password.chars().count() > 1024 {
        return Err("Enter the SSH password.".into());
    }
    Ok(())
}

pub async fn open(
    events: UnboundedSender<Event>,
    handles: SshHandles,
    console_id: String,
    host: String,
    port: u16,
    username: String,
    password: String,
) -> Result<(), String> {
    validate_credentials(&host, port, &username, &password)?;
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(60)),
        ..Default::default()
    });
    let mut session = tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, (host.as_str(), port), Client),
    )
    .await
    .map_err(|_| "The SSH connection timed out.".to_owned())?
    .map_err(|error| format!("Cannot connect to the SSH server: {error}"))?;
    let auth = session
        .authenticate_password(username.clone(), password)
        .await
        .map_err(|error| format!("SSH authentication failed: {error}"))?;
    if !auth.success() {
        return Err("SSH authentication failed. Check the user name and password.".into());
    }
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|error| format!("Cannot open an SSH session: {error}"))?;
    channel
        .request_pty(false, "xterm-256color", 100, 30, 0, 0, &[])
        .await
        .map_err(|error| format!("Cannot request an SSH terminal: {error}"))?;
    channel
        .request_shell(true)
        .await
        .map_err(|error| format!("Cannot start the SSH shell: {error}"))?;
    let (input_tx, mut input_rx) = unbounded_channel::<SshInput>();
    if !handles.register(&console_id, input_tx) {
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await;
        return Ok(());
    }
    let _ = events.send(Event::SshOpened {
        console_id: console_id.clone(),
        host: host.clone(),
        username,
    });
    let task_events = events.clone();
    let task_handles = handles.clone();
    let task_id = console_id.clone();
    tokio::spawn(async move {
        let mut message = None;
        loop {
            tokio::select! {
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                            let _ = task_events.send(Event::SshOutput {
                                console_id: task_id.clone(),
                                data: data.to_vec(),
                            });
                        }
                        Some(ChannelMsg::ExitStatus { exit_status }) => {
                            message = Some(format!("SSH shell exited with status {exit_status}."));
                        }
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                        _ => {}
                    }
                }
                input = input_rx.recv() => {
                    match input {
                        Some(SshInput::Data(data)) => {
                            if let Err(error) = channel.data(&data[..]).await {
                                message = Some(format!("SSH write failed: {error}"));
                                break;
                            }
                        }
                        Some(SshInput::Close) | None => break,
                    }
                }
            }
        }
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await;
        task_handles.remove(&task_id);
        let _ = task_events.send(Event::SshClosed {
            console_id: task_id,
            message,
        });
    });
    Ok(())
}

/// A small terminal buffer with ANSI escape filtering, CR/LF and backspace handling.
#[derive(Clone, Debug, Default)]
pub struct TerminalBuffer {
    lines: Vec<String>,
    current: String,
    filter: AnsiFilter,
    max_lines: usize,
}

impl TerminalBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            max_lines,
            ..Self::default()
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        for character in text.chars() {
            self.feed_char(character);
        }
    }

    fn feed_char(&mut self, character: char) {
        match self.filter.state {
            AnsiState::Normal => match character {
                '\x1b' => self.filter.state = AnsiState::Escape,
                '\r' => self.current.clear(),
                '\n' => self.push_line(),
                '\x08' => {
                    self.current.pop();
                }
                '\t' => {
                    let spaces = 8 - (self.current.chars().count() % 8);
                    for _ in 0..spaces {
                        self.current.push(' ');
                    }
                }
                character if character.is_control() => {}
                character => self.current.push(character),
            },
            AnsiState::Escape => match character {
                '[' => self.filter.state = AnsiState::Csi,
                ']' => self.filter.state = AnsiState::Osc,
                _ => self.filter.state = AnsiState::Normal,
            },
            AnsiState::Csi => {
                if ('\x40'..='\x7e').contains(&character) {
                    self.filter.state = AnsiState::Normal;
                }
            }
            AnsiState::Osc => {
                if character == '\x07' {
                    self.filter.state = AnsiState::Normal;
                }
            }
        }
    }

    fn push_line(&mut self) {
        let line = std::mem::take(&mut self.current);
        self.lines.push(line);
        if self.max_lines > 0 && self.lines.len() > self.max_lines {
            let excess = self.lines.len() - self.max_lines;
            self.lines.drain(0..excess);
        }
    }

    pub fn display(&self) -> String {
        let mut output = self.lines.join("\n");
        if !self.current.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&self.current);
        }
        output
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.current.clear();
        self.filter = AnsiFilter::default();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AnsiState {
    #[default]
    Normal,
    Escape,
    Csi,
    Osc,
}

#[derive(Clone, Copy, Debug, Default)]
struct AnsiFilter {
    state: AnsiState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_credentials_are_validated() {
        assert!(validate_credentials("192.168.8.10", 22, "root", "secret").is_ok());
        assert!(validate_credentials("", 22, "root", "secret").is_err());
        assert!(validate_credentials("bad host", 22, "root", "secret").is_err());
        assert!(validate_credentials("host", 0, "root", "secret").is_err());
        assert!(validate_credentials("host", 22, "", "secret").is_err());
        assert!(validate_credentials("host", 22, "root", "").is_err());
    }

    #[test]
    fn terminal_buffer_handles_ansi_crlf_backspace_and_scrollback() {
        let mut buffer = TerminalBuffer::new(3);
        buffer.feed(b"\x1b[31mhello\x1b[0m\nworld\rx\x08y!");
        assert_eq!(buffer.display(), "hello\ny!");
        buffer.feed(b"\nsecond\nthird\nfourth");
        assert_eq!(buffer.display().lines().count(), 4);
        assert!(!buffer.display().contains("hello"));
    }

    #[test]
    fn terminal_buffer_keeps_unicode_and_clears() {
        let mut buffer = TerminalBuffer::new(10);
        buffer.feed("สวัสดี\n".as_bytes());
        assert_eq!(buffer.display(), "สวัสดี");
        buffer.clear();
        assert!(buffer.display().is_empty());
    }
}
