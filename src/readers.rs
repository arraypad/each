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

