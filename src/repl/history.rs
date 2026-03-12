pub struct History {
    entries: Vec<String>,
    /// Current navigation index. `None` means we're at the bottom (new input).
    index: Option<usize>,
}

impl History {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: None,
        }
    }

    /// Record a command. Deduplicates by removing any earlier occurrence.
    pub fn push(&mut self, entry: String) {
        self.entries.retain(|e| e != &entry);
        self.entries.push(entry);
        self.reset();
    }

    /// Navigate up (older). Returns the entry to display, or None if already at top.
    pub fn up(&mut self) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        match self.index {
            None => {
                let idx = self.entries.len() - 1;
                self.index = Some(idx);
                Some(&self.entries[idx])
            }
            Some(0) => None,
            Some(idx) => {
                let new_idx = idx - 1;
                self.index = Some(new_idx);
                Some(&self.entries[new_idx])
            }
        }
    }

    /// Navigate down (newer). Returns the entry to display, or None if already at bottom.
    pub fn down(&mut self) -> Option<&str> {
        match self.index {
            None => None,
            Some(idx) => {
                if idx + 1 < self.entries.len() {
                    let new_idx = idx + 1;
                    self.index = Some(new_idx);
                    Some(&self.entries[new_idx])
                } else {
                    // Back to bottom — clear the input
                    self.index = None;
                    Some("")
                }
            }
        }
    }

    /// Reset navigation state (call after a command is submitted or input is cancelled).
    pub fn reset(&mut self) {
        self.index = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_down_navigation() {
        let mut h = History::new();
        h.push("/info".into());
        h.push("/tree".into());

        // Navigate up
        assert_eq!(h.up(), Some("/tree"));
        assert_eq!(h.up(), Some("/info"));
        assert_eq!(h.up(), None); // at top

        // Navigate back down
        assert_eq!(h.down(), Some("/tree"));
        assert_eq!(h.down(), Some("")); // back to bottom, clears input
        assert_eq!(h.down(), None); // already at bottom
    }

    #[test]
    fn deduplication() {
        let mut h = History::new();
        h.push("/info".into());
        h.push("/tree".into());
        h.push("/info".into()); // duplicate — should move to end

        assert_eq!(h.up(), Some("/info"));
        assert_eq!(h.up(), Some("/tree"));
        assert_eq!(h.up(), None); // only 2 entries
    }

    #[test]
    fn empty_history() {
        let mut h = History::new();
        assert_eq!(h.up(), None);
        assert_eq!(h.down(), None);
    }
}
