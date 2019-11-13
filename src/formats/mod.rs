mod csv;
mod json;

use failure::Error;
use indexmap::IndexMap;
use std::io::prelude::*;

use crate::formats::csv::{Csv as CsvFormat, ID as CsvId};
use crate::formats::json::{Json as JsonFormat, ID as JsonId};
use crate::readers::{CachedReader, CACHE_LEN};

pub const DEFAULT_FORMAT: &'static str = JsonId;

pub trait Format {
	fn add_arguments<'a, 'b>(&self, args: clap::App<'a, 'b>) -> clap::App<'a, 'b>;
	fn set_arguments(&mut self, matches: &clap::ArgMatches) -> Result<(), Error>;
	fn get_extensions(&self) -> &'static [&'static str];
	fn is_valid_header(&self, header: &[u8]) -> Result<bool, Error>;
	fn parse(&self, input: &mut dyn Read) -> Result<Vec<serde_json::Value>, Error>;
	fn write(&self, values: Vec<serde_json::Value>) -> Result<(), Error>;
}

pub fn load_formats() -> IndexMap<&'static str, Box<dyn Format>> {
	let mut formats: IndexMap<&'static str, Box<dyn Format>> = IndexMap::new();
	formats.insert(JsonId, Box::new(JsonFormat {}));
	formats.insert(CsvId, Box::new(CsvFormat::default()));

	formats
}

pub fn guess_format<'a>(
	ext: &Option<String>,
	reader: &mut CachedReader,
	formats: &'a IndexMap<&'static str, Box<dyn Format>>,
) -> Option<&'a Box<dyn Format>> {
	if let Some(ref ext) = ext {
		for (_, format) in formats {
			for pe in format.get_extensions() {
				if ext == pe {
					return Some(format);
				}
			}
		}
	}

	let mut header = [0; CACHE_LEN];
	if let Ok(_) = reader.read(&mut header) {
		reader.rewind();

		for (_, format) in formats {
			if let Ok(is_header) = format.is_valid_header(&header) {
				if is_header {
					return Some(format);
				}
			}
		}
	}

	None
}
