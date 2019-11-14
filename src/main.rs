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

mod action;
mod errors;
mod formats;
mod readers;
mod tests;

use clap::{AppSettings, Arg};
use dialoguer::Confirmation;
use indexmap::IndexMap;
use log::info;
use rayon::prelude::*;
use std::path::Path;

use action::Action;
use errors::EachError;
use formats::{Format, DEFAULT_FORMAT};
use readers::{CachedReader, FileReader};

fn main() {
	env_logger::init();

	let formats = formats::load_formats();
	let format_ids: Vec<&str> = formats.iter().map(|(&k, _)| k).collect();

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
				.takes_value(true)
				.possible_values(format_ids.as_slice()),
		)
		.arg(
			Arg::with_name("output-format")
				.short("F")
				.long("output-format")
				.value_name("FORMAT")
				.help("Output file format")
				.takes_value(true)
				.possible_values(format_ids.as_slice()),
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

fn run(
	args: clap::App,
	mut formats: IndexMap<&'static str, Box<dyn Format>>,
) -> Result<(), EachError> {
	let arg_matches = args.get_matches();
	info!("arguments: {:?}", arg_matches);

	for (format_id, ref mut format) in &mut formats {
		if let Err(e) = format.set_arguments(&arg_matches) {
			return Err(EachError::Usage {
				message: format!("Invalid argument for format {}: {}", format_id, e),
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
			None => match formats::guess_format(ext, reader, &formats) {
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
			None => formats.get(DEFAULT_FORMAT).unwrap(),
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
