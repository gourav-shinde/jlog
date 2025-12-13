use std::fs::File;
use std::io::{self, BufReader, BufRead, Read};
use std::path::Path;

/// A buffered file reader with various reading strategies
pub struct BufferedFileReader {
    path: String,
    buffer_size: usize,
}

impl BufferedFileReader {
    /// Create a new BufferedFileReader with default buffer size (8KB)
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_string_lossy().to_string(),
            buffer_size: 8192,
        }
    }

    /// Create a new BufferedFileReader with custom buffer size
    pub fn with_buffer_size<P: AsRef<Path>>(path: P, buffer_size: usize) -> Self {
        Self {
            path: path.as_ref().to_string_lossy().to_string(),
            buffer_size,
        }
    }

    /// Read file line by line, calling a callback for each line
    pub fn read_lines<F>(&self, mut callback: F) -> io::Result<usize>
    where
        F: FnMut(usize, &str) -> io::Result<()>,
    {
        let file = File::open(&self.path)?;
        let reader = BufReader::with_capacity(self.buffer_size, file);
        let mut line_count = 0;

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            callback(i, &line)?;
            line_count += 1;
        }

        Ok(line_count)
    }

    /// Read file in chunks, calling a callback for each chunk
    pub fn read_chunks<F>(&self, chunk_size: usize, mut callback: F) -> io::Result<u64>
    where
        F: FnMut(&[u8], usize) -> io::Result<()>,
    {
        let file = File::open(&self.path)?;
        let mut reader = BufReader::with_capacity(self.buffer_size, file);
        let mut buffer = vec![0u8; chunk_size];
        let mut total_bytes = 0u64;

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            callback(&buffer[..bytes_read], bytes_read)?;
            total_bytes += bytes_read as u64;
        }

        Ok(total_bytes)
    }

    /// Read entire file to String (use only for small files)
    pub fn read_to_string(&self) -> io::Result<String> {
        let file = File::open(&self.path)?;
        let mut reader = BufReader::with_capacity(self.buffer_size, file);
        let mut contents = String::new();
        reader.read_to_string(&mut contents)?;
        Ok(contents)
    }

    /// Read entire file to Vec<u8> (use only for small files)
    pub fn read_to_bytes(&self) -> io::Result<Vec<u8>> {
        let file = File::open(&self.path)?;
        let mut reader = BufReader::with_capacity(self.buffer_size, file);
        let mut contents = Vec::new();
        reader.read_to_end(&mut contents)?;
        Ok(contents)
    }

    /// Get file size in bytes
    pub fn file_size(&self) -> io::Result<u64> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len())
    }

    /// Check if file exists
    pub fn exists(&self) -> bool {
        Path::new(&self.path).exists()
    }

    /// Read with progress tracking
    pub fn read_lines_with_progress<F, P>(
        &self,
        mut callback: F,
        mut progress: P,
        progress_interval: usize,
    ) -> io::Result<usize>
    where
        F: FnMut(usize, &str) -> io::Result<()>,
        P: FnMut(usize, f64),
    {
        let file = File::open(&self.path)?;
        let file_size = file.metadata()?.len() as f64;
        let reader = BufReader::with_capacity(self.buffer_size, file);
        let mut line_count = 0;
        let mut bytes_processed = 0u64;

        for (i, line) in reader.lines().enumerate() {
            let line = line?;
            bytes_processed += line.len() as u64 + 1;
            callback(i, &line)?;
            line_count += 1;

            if line_count % progress_interval == 0 {
                let percent = (bytes_processed as f64 / file_size) * 100.0;
                progress(line_count, percent);
            }
        }

        Ok(line_count)
    }
}

// Example usage
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_usage() {
        // Create reader
        let reader = BufferedFileReader::new("example.txt");

        // Read line by line
        let _ = reader.read_lines(|line_num, line| {
            println!("Line {}: {}", line_num, line);
            Ok(())
        });

        // Read in chunks
        let _ = reader.read_chunks(4096, |chunk, size| {
            println!("Read {} bytes", size);
            // Process chunk here
            Ok(())
        });

        // Read with progress
        let _ = reader.read_lines_with_progress(
            |_, line| {
                // Process line
                Ok(())
            },
            |lines, percent| {
                println!("Processed {} lines ({:.1}%)", lines, percent);
            },
            1000,
        );
    }
}
