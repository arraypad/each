use failure::Error;
use std::fs::File;
use std::io::{BufReader, Read};

pub struct FileReader {
	reader: BufReader<File>,
}

impl FileReader {
	pub fn new<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Error> {
		let file = File::open(path)?;
		Ok(FileReader {
			reader: BufReader::new(file),
		})
	}
}

impl Read for FileReader {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
		self.reader.read(buf)
	}
}

pub const CACHE_LEN: usize = 4096;

pub struct CachedReader {
	buffer: Vec<u8>,
	index: usize,
	reader: Box<dyn Read>,
}

impl CachedReader {
	pub fn new(reader: Box<dyn Read>) -> Self {
		CachedReader {
			buffer: Vec::new(),
			index: 0,
			reader: reader,
		}
	}

	pub fn rewind(&mut self) {
		self.index = 0;
	}
}

impl Read for CachedReader {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
		if self.index < self.buffer.len() {
			let to_read = std::cmp::min(self.buffer.len() - self.index, buf.len());
			buf[..to_read].clone_from_slice(&self.buffer[self.index..self.index + to_read]);
			self.index += to_read;
			return Ok(to_read);
		}

		match self.reader.read(buf) {
			Ok(len) => {
				if self.index < CACHE_LEN {
					self.buffer.extend_from_slice(&buf[..len]);
				}

				self.index += len;
				Ok(len)
			}
			Err(e) => Err(e),
		}
	}
}
