use clap::Arg;
use csv::ReaderBuilder;
use failure::Error;
use std::collections::HashMap;
use std::io::Read;

use crate::{EachError, Format};

pub const ID: &'static str = "csv";

#[derive(Default)]
pub struct Csv {
	delimiter: Option<u8>,
	quote: Option<u8>,
	escape: Option<u8>,
}

impl Csv {
	fn reader_builder(&self) -> ReaderBuilder {
		let mut builder = csv::ReaderBuilder::new();

		if let Some(delimiter) = self.delimiter {
			builder.delimiter(delimiter);
		}

		if let Some(quote) = self.quote {
			builder.quote(quote);
		}

		builder.escape(self.escape);
		builder
	}
}

fn str_to_u8(s: &str) -> Result<u8, Error> {
	let first = match s.chars().next() {
		Some(c) => c,
		None => {
			return Err(EachError::Usage {
				message: "Invalid char, need at least one character".to_string(),
			}
			.into())
		}
	};

	let mut bytes = [0; 4];
	match first.encode_utf8(&mut bytes).len() {
		1 => Ok(bytes[0]),
		_ => Err(EachError::Usage {
			message: format!(
				"Invalid char, first character must be single byte: {:?}",
				&first
			),
		}
		.into()),
	}
}

const CSV_EXTS: [&'static str; 1] = ["csv"];

impl Format for Csv {
	fn add_arguments<'a, 'b>(&self, args: clap::App<'a, 'b>) -> clap::App<'a, 'b> {
		args.arg(
			Arg::with_name("csv-delimiter")
				.long("csv-delimiter")
				.value_name("CHAR")
				.help("The field delimiter to use when parsing CSV")
				.default_value(",")
				.takes_value(true),
		)
		.arg(
			Arg::with_name("csv-quote")
				.long("csv-quote")
				.value_name("CHAR")
				.help("The quote character to use when parsing CSV")
				.default_value("\"")
				.takes_value(true),
		)
		.arg(
			Arg::with_name("csv-escape")
				.long("csv-escape")
				.value_name("CHAR")
				.help("The escape character to use when parsing CSV [defaults to double quoting]")
				.takes_value(true),
		)
	}

	fn set_arguments(&mut self, matches: &clap::ArgMatches) -> Result<(), Error> {
		self.delimiter = match matches.value_of("csv-delimiter") {
			Some(ref delimiter) => Some(str_to_u8(delimiter)?),
			None => None,
		};
		self.quote = match matches.value_of("csv-quote") {
			Some(ref quote) => Some(str_to_u8(quote)?),
			None => None,
		};
		self.escape = match matches.value_of("csv-escape") {
			Some(ref escape) => Some(str_to_u8(escape)?),
			None => None,
		};
		log::info!("CSV after args: {:?}", self);

		Ok(())
	}

	fn get_extensions(&self) -> &'static [&'static str] {
		&CSV_EXTS
	}

	fn is_valid_header(&self, header: &[u8]) -> Result<bool, Error> {
		let mut reader = self.reader_builder().from_reader(header);
		let has_row: Option<Result<HashMap<String, String>, _>> = reader.deserialize().next();
		if let Some(row) = has_row {
			return Ok(row.is_ok());
		}

		Ok(false)
	}

	fn parse(&self, input: &mut dyn Read) -> Result<Vec<serde_json::Value>, Error> {
		let mut reader = self.reader_builder().from_reader(input);
		let maps: Result<Vec<HashMap<String, String>>, _> = reader.deserialize().collect();
		let values: Result<Vec<serde_json::Value>, _> =
			maps?.iter().map(|h| serde_json::to_value(h)).collect();
		Ok(values?)
	}

	fn write(&self, values: Vec<serde_json::Value>) -> Result<(), Error> {
		let mut writer = csv::Writer::from_writer(std::io::stdout());

		if let Some(obj) = values[0].as_object() {
			let header: Vec<&String> = obj.keys().collect();
			writer.serialize(&header)?;
		}

		for value in values {
			match value.as_object() {
				Some(obj) => {
					let row: Result<Vec<String>, _> = obj
						.values()
						.map(|v| match v.as_str() {
							Some(s) => Ok(s.to_owned()),
							None => serde_json::to_string(v),
						})
						.collect();
					writer.serialize(row?)?;
				}
				None => {
					let row_str = serde_json::to_string(&value)?;
					writer.serialize(&row_str)?;
				}
			}
		}

		writer.flush()?;
		Ok(())
	}
}
