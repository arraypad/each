extern crate atty;
extern crate clap;
extern crate dialoguer;
extern crate exitcode;
extern crate failure;
extern crate handlebars;
extern crate indexmap;
extern crate log;
extern crate rayon;
extern crate serde_json;
extern crate subprocess;

mod formats;
mod readers;

use clap::{AppSettings, Arg};
use dialoguer::Confirmation;
use failure::{Error, Fail};
use formats::csv::{Csv as CsvFormat, ID as CsvId};
use formats::json::{Json as JsonFormat, ID as JsonId};
use handlebars::Handlebars;
use indexmap::IndexMap;
use log::info;
use rayon::prelude::*;
use std::io::prelude::*;
use std::path::Path;
use subprocess::Exec;

use readers::{CachedReader, FileReader, CACHE_LEN};

trait Format {
	fn add_arguments<'a, 'b>(&self, args: clap::App<'a, 'b>) -> clap::App<'a, 'b>;
	fn set_arguments(&mut self, matches: &clap::ArgMatches) -> Result<(), Error>;
	fn get_extensions(&self) -> &'static [&'static str];
	fn is_valid_header(&self, header: &[u8]) -> Result<bool, Error>;
	fn parse(&self, input: &mut dyn Read) -> Result<Vec<serde_json::Value>, Error>;
	fn write(&self, values: Vec<serde_json::Value>) -> Result<(), Error>;
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
	stdin: bool,
	prompt: bool,
	prompt_stdin: bool,
	templates: Handlebars,
}

impl Action {
	pub fn new(
		command: String,
		stdin: Option<String>,
		args: Vec<String>,
		prompt: bool,
		prompt_stdin: bool,
	) -> Result<Action, Error> {
		let mut templates = Handlebars::new();
		let args: Result<Vec<String>, Error> = args
			.iter()
			.enumerate()
			.map(|(i, arg)| -> Result<String, Error> {
				let name = i.to_string();
				templates.register_template_string(&name, &arg)?;
				Ok(name)
			})
			.collect();

		if let Some(ref stdin) = stdin {
			templates.register_template_string("stdin", stdin)?;
		}

		Ok(Action {
			command: command,
			args: args?,
			stdin: stdin.is_some(),
			prompt: prompt,
			prompt_stdin: prompt_stdin,
			templates: templates,
		})
	}

	pub fn prepare(&self, value: &serde_json::Value) -> Result<Exec, Error> {
		let mut cmd = Exec::cmd(&self.command);
		for arg in &self.args {
			cmd = cmd.arg(self.templates.render(arg, value)?);
		}

		if self.stdin {
			cmd = cmd.stdin(self.templates.render("stdin", value)?.as_str());
		}

		Ok(cmd)
	}

	pub fn prompt(&self, cmd: &Exec, value: &serde_json::Value) -> Result<String, Error> {
		let cmd_str = cmd.to_cmdline_lossy();

		Ok(if self.prompt_stdin {
			let stdin = self.templates.render("stdin", value)?;
			format!("# Stdin:\n{}\n- Command:\n{}\n", &stdin, &cmd_str)
		} else {
			cmd_str
		})
	}

	pub fn run(&self, cmd: Exec) -> Result<(), Error> {
		let result = cmd.capture()?;
		std::io::stdout().write_all(&result.stdout)?;
		std::io::stderr().write_all(&result.stderr)?;
		Ok(())
	}
}

fn main() {
	env_logger::init();

	let mut formats: IndexMap<&'static str, Box<dyn Format>> = IndexMap::new();
	formats.insert(JsonId, Box::new(JsonFormat {}));
	formats.insert(CsvId, Box::new(CsvFormat::default()));

	let mut args = clap::App::new("each")
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
				.value_name("FORMAT")
				.help("Input file format")
				.takes_value(true),
		)
		.arg(
			Arg::with_name("output-format")
				.short("F")
				.long("output-format")
				.value_name("FORMAT")
				.help("Output file format")
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
		.arg(
			Arg::with_name("stdin")
				.short("s")
				.long("stdin")
				.value_name("TEMPLATE")
				.help("Template string to pass to the stdin of each process")
				.takes_value(true),
		)
		.arg(
			Arg::with_name("stdin-file")
				.short("S")
				.long("stdin-file")
				.value_name("PATH")
				.help("File containing template string to pass to the stdin of each process")
				.takes_value(true),
		)
		.arg(
			Arg::with_name("prompt-stdin")
				.long("prompt-stdin")
				.help("Include stdin template in interactive prompt (implies -p)"),
		);

	for (_, format) in &formats {
		args = format.add_arguments(args);
	}

	args = args.arg(Arg::with_name("command").multiple(true));

	std::process::exit(match run(args, formats) {
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

fn guess_format<'a>(
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

fn run(
	args: clap::App,
	mut formats: IndexMap<&'static str, Box<dyn Format>>,
) -> Result<(), EachError> {
	let arg_matches = args.get_matches();
	info!("arguments: {:?}", arg_matches);

	for (format_id, ref mut format) in &mut formats {
		if let Err(e) = format.set_arguments(&arg_matches) {
			return Err(EachError::Usage {
				message: format!("Invalid argument for format {}: {:?}", format_id, e),
			});
		}
	}

	let max_procs = if let Some(max_procs_str) = arg_matches.value_of("max-procs") {
		match max_procs_str.parse::<usize>() {
			Ok(max_procs) => max_procs,
			Err(e) => {
				return Err(EachError::Usage {
					message: format!("Invalid max-procs: {} ({})", &max_procs_str, e),
				})
			}
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

			let stdin = match arg_matches.value_of("stdin") {
				Some(stdin) => Some(stdin.to_string()),
				None => match arg_matches.value_of("stdin-file") {
					Some(stdin_file) => Some(std::fs::read_to_string(stdin_file)?),
					None => None,
				},
			};

			let prompt_stdin = arg_matches.is_present("prompt-stdin");

			match Action::new(
				command,
				stdin,
				commands.map(|c| c.to_string()).collect(),
				prompt_stdin || arg_matches.is_present("prompt"),
				prompt_stdin,
			) {
				Ok(action) => Some(action),
				Err(e) => {
					return Err(EachError::Usage {
						message: format!("Invalid template: {:?}", e),
					})
				}
			}
		}
		None => None,
	};

	let mut output_values = Vec::new();

	for (ref ext, ref mut reader) in readers.iter_mut() {
		let format = match arg_matches.value_of("format") {
			Some(format_id) => match formats.get(format_id) {
				Some(format) => format,
				None => {
					return Err(EachError::Usage {
						message: format!("Unknown format: {}", &format_id),
					})
				}
			},
			None => match guess_format(ext, reader, &formats) {
				Some(format) => format,
				None => {
					return Err(EachError::Data {
						message: format!("Unable to guess format for input"),
					})
				}
			},
		};

		let values = match format.parse(reader) {
			Ok(values) => values,
			Err(e) => {
				return Err(EachError::Data {
					message: format!("failed to parse input: {}", e),
				})
			}
		};

		match action {
			Some(ref action) => process(&values, &action)?,
			None => output_values.extend_from_slice(&values),
		}
	}

	if action.is_none() {
		let format = match arg_matches.value_of("output-format") {
			Some(format_id) => match formats.get(format_id) {
				Some(format) => format,
				None => {
					return Err(EachError::Usage {
						message: format!("Unknown output format: {}", &format_id),
					})
				}
			},
			None => formats.get(JsonId).unwrap(),
		};

		if let Err(e) = format.write(output_values) {
			return Err(EachError::Data {
				message: format!("serialize error: {:?}", e),
			});
		}
	}

	Ok(())
}

fn process(values: &Vec<serde_json::Value>, action: &Action) -> Result<(), EachError> {
	let results: Result<Vec<()>, EachError> = values
		.par_iter()
		.map(|ref value| -> Result<(), EachError> {
			match action.prepare(value) {
				Ok(cmd) => {
					let run = if action.prompt {
						let prompt = match action.prompt(&cmd, &value) {
							Ok(prompt) => prompt,
							Err(e) => {
								return Err(EachError::Data {
									message: format!("failed to render stdin: {:?}", e),
								})
							}
						};
						Confirmation::new().with_text(&prompt).interact()?
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
				}
				Err(e) => {
					return Err(EachError::Data {
						message: format!("failed to prepare command: {:?}", e),
					})
				}
			}
		})
		.collect();

	match results {
		Ok(_) => Ok(()),
		Err(e) => Err(e),
	}
}
