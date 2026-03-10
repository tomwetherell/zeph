use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::style::{Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};

use super::style;

const FRAMES: &[char] = &['·', '✻', '✶', '✳', '✢'];
const FRAME_INTERVAL: Duration = Duration::from_millis(120);

/// An animated spinner that runs on a background thread.
///
/// Call [`Spinner::start`] to begin, then [`Spinner::stop_with_message`]
/// to freeze the line and print subtitle lines underneath.
pub struct Spinner {
    stop_flag: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Spinner {
    /// Start a spinner with a label (pink) and optional detail (black, in parens).
    ///
    /// The spinner prints on its own line and cycles through the frame
    /// characters at the front of the line.
    pub fn start(label: &str, detail: Option<&str>) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flag = stop_flag.clone();
        let label = label.to_string();
        let detail = detail.map(|s| s.to_string());

        let handle = thread::spawn(move || {
            let mut out = io::stdout();
            let mut idx = 0usize;
            loop {
                let frame = FRAMES[idx % FRAMES.len()];
                let _ = crossterm::execute!(
                    out,
                    Print("\r"),
                    Clear(ClearType::CurrentLine),
                    SetForegroundColor(style::HEADING),
                    Print(format!("  {frame} {label}")),
                );
                if let Some(ref d) = detail {
                    let _ = crossterm::execute!(
                        out,
                        SetForegroundColor(style::DIM_DARK),
                        Print(format!(" ({d})")),
                    );
                }
                let _ = crossterm::execute!(out, ResetColor);
                let _ = out.flush();

                // Sleep in small increments so we can respond to stop quickly.
                let steps = 6;
                let step_dur = FRAME_INTERVAL / steps;
                for _ in 0..steps {
                    if flag.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(step_dur);
                }
                idx += 1;
            }
        });

        Spinner {
            stop_flag,
            handle: Some(handle),
        }
    }

    /// Stop the spinner, freeze the line, and print subtitle messages
    /// underneath using the `⎿` prefix style.
    pub fn stop_with_message(mut self, subtitles: &[&str]) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }

        let mut out = io::stdout();
        // Move past the spinner line
        let _ = crossterm::execute!(out, Print("\n"));

        for msg in subtitles {
            let _ = crossterm::execute!(
                out,
                SetForegroundColor(style::DIM),
                Print(format!("  ⎿  {msg}\n")),
                ResetColor,
            );
        }
        let _ = out.flush();
    }

}

impl Drop for Spinner {
    fn drop(&mut self) {
        // Ensure the thread stops even if neither stop method was called.
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
