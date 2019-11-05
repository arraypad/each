use failure::Error;
use std::io::Read;

use crate::Parser;

pub struct Json {}

const JSON_EXTS: [&'static str; 1] = ["json"];

impl Parser for Json {
	fn get_extensions(&self) -> &'static [&'static str] {
		&JSON_EXTS
	}

	fn is_valid_header(&self) -> Result<bool, Error> {
		Ok(true)
	}

	fn parse(
		&self,
		input: &mut dyn Read,
	) -> Result<Box<dyn Iterator<Item = serde_json::Value>>, Error> {
		// read to string first - see https://github.com/serde-rs/json/issues/160
		let mut buffer = String::new();
		input.read_to_string(&mut buffer)?;

		let records: Vec<serde_json::Value> = serde_json::from_str(&buffer)?;
		Ok(Box::new(records.into_iter()))
	}
}
