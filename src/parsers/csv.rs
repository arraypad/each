use failure::Error;
use std::io::Read;
use std::collections::HashMap;

use crate::Parser;

pub const ID: &'static str = "csv";

pub struct Csv {}

const CSV_EXTS: [&'static str; 1] = ["csv"];

impl Parser for Csv {
	fn get_extensions(&self) -> &'static [&'static str] {
		&CSV_EXTS
	}

	fn is_valid_header(&self, header: &[u8]) -> Result<bool, Error> {
		let mut reader = csv::Reader::from_reader(header);
		let has_row: Option<Result<HashMap<String, String>, _>> = reader.deserialize().next();
		if let Some(row) = has_row {
			return Ok(row.is_ok());
		}

		Ok(false)
	}

	fn parse(
		&self,
		input: &mut dyn Read,
	) -> Result<Vec<serde_json::Value>, Error> {
		let mut reader = csv::Reader::from_reader(input);
		let maps: Result<Vec<HashMap<String, String>>, _> = reader.deserialize().collect();
		let values: Result<Vec<serde_json::Value>, _> = maps?.iter().map(|h| serde_json::to_value(h)).collect();
		Ok(values?)
	}
}
