//! Log streaming — capture stdout/stderr from running commands.

use crate::error::RunnerError;
use adp_ci_types::LogStream;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Manages log streaming for a job.
///
/// Captures stdout/stderr output and accumulates it for:
/// - Real-time streaming to adp via SSE
/// - Historical retrieval from git refs
pub struct LogStreamer {
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    stdout_offset: u64,
    stderr_offset: u64,
}

impl LogStreamer {
    /// Create a new log streamer for a job, writing to the given directory.
    pub fn new(log_dir: &Path) -> Result<Self, RunnerError> {
        std::fs::create_dir_all(log_dir)?;
        Ok(Self {
            stdout_path: log_dir.join("stdout.log"),
            stderr_path: log_dir.join("stderr.log"),
            stdout_offset: 0,
            stderr_offset: 0,
        })
    }

    /// Append data to the given stream.
    pub fn append(&mut self, stream: LogStream, data: &[u8]) -> Result<u64, RunnerError> {
        let (path, offset) = match stream {
            LogStream::Stdout => (&self.stdout_path, &mut self.stdout_offset),
            LogStream::Stderr => (&self.stderr_path, &mut self.stderr_offset),
        };

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        file.write_all(data)?;
        *offset += data.len() as u64;

        Ok(*offset)
    }

    /// Append a line to the given stream.
    pub fn append_line(&mut self, stream: LogStream, line: &str) -> Result<u64, RunnerError> {
        let data = format!("{}\n", line);
        self.append(stream, data.as_bytes())
    }

    /// Get current offset for a stream.
    pub fn offset(&self, stream: LogStream) -> u64 {
        match stream {
            LogStream::Stdout => self.stdout_offset,
            LogStream::Stderr => self.stderr_offset,
        }
    }

    /// Read all accumulated data for a stream.
    pub fn read_all(&self, stream: LogStream) -> Result<String, RunnerError> {
        let path = match stream {
            LogStream::Stdout => &self.stdout_path,
            LogStream::Stderr => &self.stderr_path,
        };

        if path.exists() {
            Ok(std::fs::read_to_string(path)?)
        } else {
            Ok(String::new())
        }
    }

    /// Read from offset for a stream (for historical retrieval).
    pub fn read_from(&self, stream: LogStream, offset: u64) -> Result<String, RunnerError> {
        let content = self.read_all(stream)?;
        let content_bytes = content.as_bytes();
        if (offset as usize) >= content_bytes.len() {
            return Ok(String::new());
        }
        Ok(content[offset as usize..].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_read() {
        let tmp = tempfile::tempdir().unwrap();
        let mut streamer = LogStreamer::new(tmp.path()).unwrap();

        streamer
            .append_line(LogStream::Stdout, "Building project...")
            .unwrap();
        streamer
            .append_line(LogStream::Stdout, "Compiling main.rs")
            .unwrap();

        let content = streamer.read_all(LogStream::Stdout).unwrap();
        assert!(content.contains("Building project..."));
        assert!(content.contains("Compiling main.rs"));
    }

    #[test]
    fn stderr_separate_stream() {
        let tmp = tempfile::tempdir().unwrap();
        let mut streamer = LogStreamer::new(tmp.path()).unwrap();

        streamer
            .append_line(LogStream::Stdout, "stdout content")
            .unwrap();
        streamer
            .append_line(LogStream::Stderr, "stderr content")
            .unwrap();

        let stdout = streamer.read_all(LogStream::Stdout).unwrap();
        let stderr = streamer.read_all(LogStream::Stderr).unwrap();
        assert_eq!(stdout, "stdout content\n");
        assert_eq!(stderr, "stderr content\n");
    }

    #[test]
    fn offset_tracking() {
        let tmp = tempfile::tempdir().unwrap();
        let mut streamer = LogStreamer::new(tmp.path()).unwrap();

        assert_eq!(streamer.offset(LogStream::Stdout), 0);

        streamer
            .append(LogStream::Stdout, b"hello")
            .unwrap();
        assert_eq!(streamer.offset(LogStream::Stdout), 5);

        streamer
            .append(LogStream::Stdout, b" world")
            .unwrap();
        assert_eq!(streamer.offset(LogStream::Stdout), 11);
    }

    #[test]
    fn read_from_offset() {
        let tmp = tempfile::tempdir().unwrap();
        let mut streamer = LogStreamer::new(tmp.path()).unwrap();

        streamer
            .append(LogStream::Stdout, b"hello world")
            .unwrap();

        let from_6 = streamer.read_from(LogStream::Stdout, 6).unwrap();
        assert_eq!(from_6, "world");
    }

    #[test]
    fn read_empty_stream() {
        let tmp = tempfile::tempdir().unwrap();
        let streamer = LogStreamer::new(tmp.path()).unwrap();

        let content = streamer.read_all(LogStream::Stdout).unwrap();
        assert!(content.is_empty());
    }
}
