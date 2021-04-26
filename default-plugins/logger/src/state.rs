use chrono::prelude::*;
use std::fmt;
use zellij_tile::prelude::LogLevel;

pub struct Message {
    content: String,
    log_level: LogLevel,
    timestamp: DateTime<Local>,
}
impl Message {
    pub fn new(content: String, log_level: Option<LogLevel>) -> Self {
        Message {
            content,
            log_level: log_level.unwrap_or_default(),
            timestamp: Local::now(),
        }
    }
}

#[derive(Default)]
pub struct State {
    message_history: Vec<Message>,
    index: usize,
}
impl State {
    pub fn append_message(&mut self, content: String, log_level: Option<LogLevel>) {
        self.message_history.push(Message::new(content, log_level));
    }
    pub fn inc_index(&mut self, count: Option<usize>) -> usize {
        let count = count.unwrap_or(1);
        let last_message_index = self.message_history.len() - 1;

        self.index = if self.index + count >= last_message_index {
            last_message_index
        } else {
            self.index + count
        };
        self.index
    }
    pub fn dec_index(&mut self, count: Option<usize>) -> usize {
        let count = count.unwrap_or(1);

        self.index = if self.index <= count {
            0
        } else {
            self.index - count
        };
        self.index
    }
    pub fn get_index(&self) -> usize {
        self.index
    }
    pub fn get_current_message(&self) -> Option<&Message> {
        self.message_history.get(self.index)
    }
    pub fn get_message_count(&self) -> usize {
        self.message_history.len()
    }
}
impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(msg) = self.get_current_message() {
            let index = self.get_index() + 1;
            writeln!(
                f,
                "{}/{} ({}%)",
                index,
                self.get_message_count(),
                (index as f32 / (self.get_message_count() as f32)) * 100.
            )?;
            writeln!(f, "Level: {}", msg.log_level)?;
            writeln!(f, "Message: {}", msg.content)?;
            writeln!(f, "@{}", msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string())?;
        } else {
            writeln!(f, "All good!")?;
        }

        Ok(())
    }
}
