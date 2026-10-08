//! Minimal single-line text input.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Default)]
pub struct LineInput {
    chars: Vec<char>,
    /// Cursor position in chars.
    pub cursor: usize,
}

impl LineInput {
    pub fn set(&mut self, s: &str) {
        self.chars = s.chars().collect();
        self.cursor = self.chars.len();
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn handle(&mut self, ev: &KeyEvent) {
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        match ev.code {
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.chars.len(),
            KeyCode::Char('u') if ctrl => {
                self.chars.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let mut start = self.cursor;
                while start > 0 && self.chars[start - 1] == ' ' {
                    start -= 1;
                }
                while start > 0 && self.chars[start - 1] != ' ' {
                    start -= 1;
                }
                self.chars.drain(start..self.cursor);
                self.cursor = start;
            }
            KeyCode::Char(c) if !ctrl => {
                self.chars.insert(self.cursor, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.chars.remove(self.cursor);
            }
            KeyCode::Delete if self.cursor < self.chars.len() => {
                self.chars.remove(self.cursor);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.chars.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.chars.len(),
            _ => {}
        }
    }
}
