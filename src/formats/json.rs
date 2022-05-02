use failure::Error;
use std::io::Read;

use crate::formats::Format;

pub const ID: &str = "json";

pub struct Json {}

const JSON_EXTS: [&str; 1] = ["json"];

impl Format for Json {
	fn add_arguments<'a>(&self, args: clap::Command<'a>) -> clap::Command<'a> {
		args
	}

	fn set_arguments(&mut self, _matches: &clap::ArgMatches) -> Result<(), Error> {
		Ok(())
	}

	fn get_extensions(&self) -> &'static [&'static str] {
		&JSON_EXTS
	}

	fn is_valid_header(&self, header: &[u8]) -> Result<bool, Error> {
		Ok(header[0] as char == '[' || header[0] as char == '{')
	}

	fn parse(&self, input: &mut dyn Read) -> Result<serde_json::Value, Error> {
		// read to string first - see https://github.com/serde-rs/json/issues/160
		let mut buffer = String::new();
		input.read_to_string(&mut buffer)?;

		Ok(serde_json::from_str(&buffer)?)
	}

	fn write(&self, values: Vec<serde_json::Value>) -> Result<(), Error> {
		Ok(serde_json::to_writer_pretty(std::io::stdout(), &values)?)
	}
}
