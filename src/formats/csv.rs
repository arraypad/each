use failure::Error;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::io::Read;

use crate::Format;

pub const ID: &'static str = "csv";

pub struct Csv {}

const CSV_EXTS: [&'static str; 1] = ["csv"];

impl Format for Csv {
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

	fn write(&self, values: Vec<serde_json::Value>) -> Result<(), Error> {
		let mut writer = csv::Writer::from_writer(std::io::stdout());

		if let Some(obj) = values[0].as_object() {
			let header: Vec<&String> = obj.keys().collect();
			writer.serialize(&header)?;
		}

		for value in values {
			if value.is_object() {
				let row: Result<Vec<String>, _> = value.as_object()
					.unwrap()
					.values()
					.map(|v| match v.as_str() {
						Some(s) => Ok(s.to_owned()),
						None => serde_json::to_string(v),
					})
					.collect();
				writer.serialize(row?)?;
			} else {
				let row_str = serde_json::to_string(&value)?;
				writer.serialize(&row_str)?;
			}
		}

		writer.flush()?;
		Ok(())
	}
}
