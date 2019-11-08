extern crate atty;
extern crate clap;
extern crate dialoguer;
extern crate exitcode;
extern crate failure;
extern crate handlebars;
extern crate log;
extern crate rayon;
extern crate serde_json;
extern crate subprocess;

mod parsers;
mod readers;

use clap::{AppSettings, Arg};
use dialoguer::Confirmation;
use failure::{Error, Fail};
use handlebars::Handlebars;
use log::info;
use parsers::json::{Json as JsonParser, ID as JsonId};
use parsers::csv::{Csv as CsvParser, ID as CsvId};
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::prelude::*;
use std::path::Path;
use subprocess::Exec;

use readers::{CachedReader, FileReader, CACHE_LEN};

trait Parser {
	fn get_extensions(&self) -> &'static [&'static str];
	fn is_valid_header(&self, header: &[u8]) -> Result<bool, Error>;
	fn parse(
		&self,
		input: &mut dyn Read,
	) -> Result<Vec<serde_json::Value>, Error>;
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

struct Action {
	command: String,
	args: Vec<String>,
	prompt: bool,
}

impl Action {
	pub fn prepare(&self, value: &serde_json::Value) -> Result<Exec, Error> {
		let reg = Handlebars::new();

		let mut cmd = Exec::cmd(&self.command);
		for arg in &self.args {
			cmd = cmd.arg(reg.render_template(arg, value)?);
		}

		Ok(cmd)
	}

	pub fn run(&self, cmd: Exec) -> Result<(), Error> {
		cmd.join()?;
		Ok(())
	}
}

fn main() {
	env_logger::init();

	let mut parsers: HashMap<&'static str, Box<dyn Parser>> = HashMap::new();
	parsers.insert(JsonId, Box::new(JsonParser {}));
	parsers.insert(CsvId, Box::new(CsvParser {}));

	let args = clap::App::new("each")
		.version("0.1")
		.author("Arpad Ray <arraypad@gmail.com>")
		.about("Build and execute command lines from structured input")
		.setting(AppSettings::TrailingVarArg)
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
		)
		.arg(
			Arg::with_name("prompt")
				.short("p")
				.long("interactive")
				.help("Prompt for each value"),
		)
		.arg(
			Arg::with_name("max-procs")
				.short("P")
				.long("max-procs")
				.value_name("max-procs")
				.help("Run up to max-procs processes at a time")
				.takes_value(true),
		)
		.arg(Arg::with_name("command").multiple(true));

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

fn guess_parser<'a>(
	ext: &Option<String>,
	reader: &mut CachedReader,
	parsers: &'a HashMap<&'static str, Box<dyn Parser>>,
) -> Option<&'a Box<dyn Parser>> {
	if let Some(ref ext) = ext {
		for (_, parser) in parsers {
			for pe in parser.get_extensions() {
				if ext == pe {
					return Some(parser);
				}
			}
		}
	}

	let mut header = [0; CACHE_LEN];
	if let Ok(_) = reader.read(&mut header) {
		reader.rewind();

		for (_, parser) in parsers {
			if let Ok(is_header) = parser.is_valid_header(&header) {
				if is_header {
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

	let max_procs = if let Some(max_procs_str) = arg_matches.value_of("max-procs") {
		match max_procs_str.parse::<usize>() {
			Ok(max_procs) => max_procs,
			Err(e) => return Err(EachError::Usage {
				message: format!("Invalid max-proces: {} ({})", &max_procs_str, e),
			}),
		}
	} else {
		1
	};

	rayon::ThreadPoolBuilder::new()
		.num_threads(max_procs)
		.build_global()
		.expect("build_global already called");

	let mut readers: Vec<(Option<String>, CachedReader)> = Vec::new();

	if let Some(input_paths) = arg_matches.values_of("input") {
		for input_path in input_paths {
			let path = Path::new(&input_path);
			let ext = if let Some(ext) = path.extension() {
				Some(ext.to_string_lossy().to_string())
			} else {
				None
			};

			let reader = match FileReader::new(&input_path) {
				Ok(reader) => Box::new(reader),
				Err(e) => {
					return Err(EachError::Data {
						message: format!("Couldn't open file {}: {}", &input_path, e),
					})
				}
			};

			let cached = CachedReader::new(reader);

			readers.push((ext, cached));
		}
	} else if atty::is(atty::Stream::Stdin) {
		return Err(EachError::Usage {
			message: "No input provided".to_owned(),
		});
	} else {
		let reader = Box::new(std::io::stdin());
		let cached = CachedReader::new(reader);
		readers.push((None, cached));
	}

	let action: Option<Action> = match arg_matches.values_of("command") {
		Some(ref mut commands) => {
			let command = match commands.next() {
				Some(command) => command.to_string(),
				None => unreachable!(),
			};

			Some(Action {
				command: command,
				args: commands.map(|c| c.to_string()).collect(),
				prompt: arg_matches.is_present("prompt"),
			})
		}
		None => None,
	};

	for (ref ext, ref mut reader) in readers.iter_mut() {
		let parser = match arg_matches.value_of("format") {
			Some(format) => match parsers.get(format) {
				Some(parser) => parser,
				None => {
					return Err(EachError::Usage {
						message: format!("Unknown format: {}", &format),
					})
				}
			},
			None => match guess_parser(ext, reader, &parsers) {
				Some(parser) => parser,
				None => {
					return Err(EachError::Data {
						message: format!("Unable to guess format for input"),
					})
				}
			},
		};

		process(reader, parser, &action)?;
}

	Ok(())
}

fn process(
	input: &mut dyn Read,
	parser: &Box<dyn Parser>,
	action: &Option<Action>,
) -> Result<(), EachError> {
	let values = match parser.parse(input) {
		Ok(values) => values,
		Err(e) => {
			return Err(EachError::Data {
				message: format!("failed to parse input: {}", e),
			})
		}
	};

	match action {
		Some(ref action) => {
			let results: Result<Vec<()>, EachError> = values.par_iter().map(|ref value| -> Result<(), EachError> {
				match action.prepare(value) {
					Ok(cmd) => {
						let run = if action.prompt {
							let cmd_str = cmd.to_cmdline_lossy();
							Confirmation::new().with_text(&cmd_str).interact()?
						} else {
							true
						};

						if run {
							if let Err(e) = action.run(cmd) {
								return Err(EachError::Data {
									message: format!("failed to run command: {:?}", e),
								});
							}
						}

						Ok(())
					},
					Err(e) => return Err(EachError::Data {
						message: format!("failed to prepare command: {:?}", e),
					}),
				}
			}).collect();

			match results {
				Ok(_) => Ok(()),
				Err(e) => Err(e),
			}
		},
		None => {
			if let Err(e) = serde_json::to_writer_pretty(std::io::stdout(), &values) {
				return Err(EachError::Data {
					message: format!("serialize error: {:?}", e),
				});
			}

			Ok(())
		}
	}
}
