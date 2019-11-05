extern crate atty;
extern crate clap;
extern crate exitcode;
extern crate failure;
extern crate log;
extern crate serde_json;

mod parsers;
mod readers;

use clap::Arg;
use failure::{Error, Fail};
use log::info;
use parsers::json::Json as JsonParser;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::prelude::*;
use std::iter::Iterator;
use std::path::Path;

use readers::FileReader;

trait Parser {
	fn get_extensions(&self) -> &'static [&'static str];
	fn is_valid_header(&self) -> Result<bool, Error>;
	fn parse(
		&self,
		input: &mut dyn Read,
	) -> Result<Box<dyn Iterator<Item = serde_json::Value>>, Error>;
}

#[derive(Debug, Fail)]
enum EachError {
	#[fail(display = "Incorrect usage: {:?}", message)]
	Usage { message: String },
	#[fail(display = "Data error: {:?}", message)]
	Data { message: String },
	#[fail(display = "IO error: {:?}", inner)]
	Io { inner: std::io::Error },
}

impl From<std::io::Error> for EachError {
	fn from(error: std::io::Error) -> Self {
		EachError::Io { inner: error }
	}
}

fn main() {
	env_logger::init();

	let mut parsers: HashMap<&'static str, Box<dyn Parser>> = HashMap::new();
	parsers.insert("json", Box::new(JsonParser {}));

	let args = clap::App::new("each")
		.version("0.1")
		.author("Arpad Ray <arraypad@gmail.com>")
		.about("Build and execute command lines from structured input")
		.arg(
			Arg::with_name("input")
				.short("i")
				.long("input")
				.value_name("FILE")
				.multiple(true)
				.help("Read input from FILE instead of stdin")
				.takes_value(true),
		)
		.arg(
			Arg::with_name("format")
				.short("f")
				.long("format")
				.value_name("FILE")
				.help("Input file format")
				.takes_value(true),
		);

	std::process::exit(match run(args, parsers) {
		Ok(_) => exitcode::OK,
		Err(e) => {
			eprintln!("Error: {}", e);
			match e {
				EachError::Usage { message: _ } => exitcode::USAGE,
				EachError::Data { message: _ } => exitcode::DATAERR,
				EachError::Io { inner: _ } => exitcode::IOERR,
			}
		}
	})
}

fn guess_parser<'a>(ext: &Option<String>, parsers: &'a HashMap<&'static str, Box<dyn Parser>>) -> Option<&'a Box<dyn Parser>> {
	if let Some(ref ext) = ext {
		for (_, parser) in parsers {
			for pe in parser.get_extensions() {
				if ext == pe {
					return Some(parser);
				}
			}
		}
	}

	None
}

fn run(args: clap::App, parsers: HashMap<&'static str, Box<dyn Parser>>) -> Result<(), EachError> {
	let arg_matches = args.get_matches();
	info!("arguments: {:?}", arg_matches);

	let mut readers: Vec<(Option<String>, Box<dyn std::io::Read>)> = Vec::new();

	if let Some(input_paths) = arg_matches.values_of("input") {
		for input_path in input_paths {
			let path = Path::new(&input_path);
			let ext = if let Some(ext) = path.extension() {
				Some(ext.to_string_lossy().to_string())
			} else {
				None
			};

			let reader = match FileReader::new(&input_path) {
				Ok(reader) => reader,
				Err(e) => return Err(EachError::Data {
					message: format!("Couldn't open file {}: {}", &input_path, e),
				}),
			};

			readers.push((ext, Box::new(reader)));
		}
	} else if atty::is(atty::Stream::Stdin) {
		return Err(EachError::Usage {
			message: "No input provided".to_owned(),
		});
	} else {
		readers.push((None, Box::new(std::io::stdin())));
	}

	for (ext, mut reader) in (&mut readers).iter() {
		let parser = match arg_matches.value_of("format") {
			Some(format) => match parsers.get(format) {
				Some(parser) => parser,
				None => return Err(EachError::Usage {
					message: format!("Unknown format: {}", &format),
				}),
			},
			None => match guess_parser(ext, &parsers) {
				Some(parser) => parser,
				None => return Err(EachError::Data {
					message: format!("Unable to guess format for input"),
				}),
			},
		};

		process(&mut reader, parser)?;
	}

	Ok(())
}

fn process(input: &mut dyn Read, parser: &Box<dyn Parser>) -> Result<(), EachError> {
	let records = match parser.parse(input) {
		Ok(records) => records,
		Err(e) => {
			return Err(EachError::Data {
				message: format!("Parse error: {}", e),
			})
		}
	};

	for record in records {
		println!("Record: {:?}", record);
	}

	Ok(())
}

