//! Undo/Redo history stack for graph operations.
//!
//! Tracks undoable actions (link creation/removal, link toggling, node moves)
//! and allows stepping backwards and forwards through the history.

use super::commands::{AppCommand, UiCommand};

/// An action that can be undone or redone.
#[derive(Debug, Clone)]
pub enum UndoAction {
    /// A PipeWire command.
    AppCommand(AppCommand),
    /// A local UI command.
    UiCommand(UiCommand),
    /// Remove the link between two ports (resolved at execution time).
    /// Used as the reverse of CreateLink since we don't know the LinkId upfront.
    RemoveLinkBetweenPorts {
        /// Source port.
        output_port: crate::util::id::PortId,
        /// Destination port.
        input_port: crate::util::id::PortId,
    },
    /// Multiple actions executed together.
    Batch(Vec<UndoAction>),
}

/// A single entry in the undo history.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    /// Human-readable description of the action.
    #[cfg_attr(not(test), allow(dead_code))]
    pub description: String,
    /// The action to execute on redo (or initial execution).
    pub forward: UndoAction,
    /// The action to execute on undo.
    pub reverse: UndoAction,
}

/// Undo/redo history stack.
pub struct UndoStack {
    /// All history entries.
    history: Vec<UndoEntry>,
    /// Points past the last executed entry (i.e. the next redo position).
    cursor: usize,
    /// Maximum number of entries to keep.
    max_size: usize,
}

impl UndoStack {
    /// Creates a new undo stack with the given capacity.
    pub fn new(max_size: usize) -> Self {
        Self {
            history: Vec::new(),
            cursor: 0,
            max_size,
        }
    }

    /// Pushes a new entry, discarding any redo history beyond the cursor.
    pub fn push(&mut self, entry: UndoEntry) {
        // Truncate redo history
        self.history.truncate(self.cursor);
        self.history.push(entry);
        self.cursor = self.history.len();

        // Trim from the front if we exceed max_size
        if self.history.len() > self.max_size {
            let excess = self.history.len() - self.max_size;
            self.history.drain(..excess);
            self.cursor = self.history.len();
        }
    }

    /// Returns the reverse action to undo the last operation, advancing the cursor back.
    pub fn undo(&mut self) -> Option<UndoAction> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        Some(self.history[self.cursor].reverse.clone())
    }

    /// Returns the forward action to redo, advancing the cursor forward.
    pub fn redo(&mut self) -> Option<UndoAction> {
        if self.cursor >= self.history.len() {
            return None;
        }
        let action = self.history[self.cursor].forward.clone();
        self.cursor += 1;
        Some(action)
    }

    /// Whether an undo operation is available.
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    /// Whether a redo operation is available.
    pub fn can_redo(&self) -> bool {
        self.cursor < self.history.len()
    }

    /// Description of the action that would be undone.
    #[cfg(test)]
    pub fn undo_description(&self) -> Option<&str> {
        if self.cursor == 0 {
            return None;
        }
        Some(&self.history[self.cursor - 1].description)
    }

    /// Description of the action that would be redone.
    #[cfg(test)]
    pub fn redo_description(&self) -> Option<&str> {
        if self.cursor >= self.history.len() {
            return None;
        }
        Some(&self.history[self.cursor].description)
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::id::{LinkId, PortId};

    fn dummy_entry(desc: &str) -> UndoEntry {
        UndoEntry {
            description: desc.to_string(),
            forward: UndoAction::AppCommand(AppCommand::CreateLink {
                output_port: PortId::new(1),
                input_port: PortId::new(2),
            }),
            reverse: UndoAction::AppCommand(AppCommand::RemoveLink(LinkId::new(10))),
        }
    }

    #[test]
    fn test_push_and_undo() {
        let mut stack = UndoStack::new(10);
        assert!(!stack.can_undo());

        stack.push(dummy_entry("test"));
        assert!(stack.can_undo());
        assert!(!stack.can_redo());

        let action = stack.undo();
        assert!(action.is_some());
        assert!(!stack.can_undo());
        assert!(stack.can_redo());
    }

    #[test]
    fn test_redo() {
        let mut stack = UndoStack::new(10);
        stack.push(dummy_entry("test"));
        stack.undo();

        let action = stack.redo();
        assert!(action.is_some());
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn test_push_truncates_redo() {
        let mut stack = UndoStack::new(10);
        stack.push(dummy_entry("first"));
        stack.push(dummy_entry("second"));
        stack.undo(); // undo "second"

        // Push a new entry — "second" should be gone
        stack.push(dummy_entry("third"));
        assert_eq!(stack.history.len(), 2);
        assert_eq!(stack.undo_description(), Some("third"));
    }

    #[test]
    fn test_max_size() {
        let mut stack = UndoStack::new(3);
        for i in 0..5 {
            stack.push(dummy_entry(&format!("entry {}", i)));
        }
        assert_eq!(stack.history.len(), 3);
        assert_eq!(stack.undo_description(), Some("entry 4"));
    }

    #[test]
    fn test_descriptions() {
        let mut stack = UndoStack::new(10);
        assert_eq!(stack.undo_description(), None);
        assert_eq!(stack.redo_description(), None);

        stack.push(dummy_entry("create link"));
        assert_eq!(stack.undo_description(), Some("create link"));

        stack.undo();
        assert_eq!(stack.redo_description(), Some("create link"));
    }
}
