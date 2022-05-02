mod action;
mod errors;
mod formats;
mod readers;
mod tests;

use clap::{Arg, Command};
use dialoguer::Confirm;
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

	let mut args = Command::new("each")
		.version("0.1")
		.author("Arpad Ray <arraypad@gmail.com>")
		.about("Build and execute command lines from structured input")
		.trailing_var_arg(true)
		.arg(
			Arg::new("input")
				.short('i')
				.long("input")
				.value_name("FILE")
				.multiple_occurrences(true)
				.help("Read input from FILE instead of stdin")
				.takes_value(true),
		)
		.arg(
			Arg::new("format")
				.short('f')
				.long("format")
				.value_name("FORMAT")
				.help("Input file format")
				.takes_value(true)
				.possible_values(format_ids.as_slice()),
		)
		.arg(
			Arg::new("query")
				.short('q')
				.long("query")
				.value_name("QUERY")
				.help("JMES query to apply to each input file")
				.takes_value(true),
		)
		.arg(
			Arg::new("output-format")
				.short('F')
				.long("output-format")
				.value_name("FORMAT")
				.help("Output file format")
				.takes_value(true)
				.possible_values(format_ids.as_slice()),
		)
		.arg(
			Arg::new("prompt")
				.short('p')
				.long("interactive")
				.help("Prompt for each value"),
		)
		.arg(
			Arg::new("max-procs")
				.short('P')
				.long("max-procs")
				.value_name("max-procs")
				.help("Run up to max-procs processes at a time")
				.takes_value(true),
		)
		.arg(
			Arg::new("stdin")
				.short('s')
				.long("stdin")
				.value_name("TEMPLATE")
				.help("Template string to pass to the stdin of each process")
				.takes_value(true),
		)
		.arg(
			Arg::new("stdin-file")
				.short('S')
				.long("stdin-file")
				.value_name("PATH")
				.help("File containing template string to pass to the stdin of each process")
				.takes_value(true),
		)
		.arg(
			Arg::new("prompt-stdin")
				.long("prompt-stdin")
				.help("Include stdin template in interactive prompt (implies -p)"),
		);

	for (_, format) in &formats {
		args = format.add_arguments(args);
	}

	args = args.arg(Arg::new("command").multiple_occurrences(true));

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
	args: Command,
	mut formats: IndexMap<&'static str, Box<dyn Format>>,
) -> Result<(), EachError> {
	let arg_matches = args.get_matches();
	info!("arguments: {:?}", arg_matches);

	for (format_id, ref mut format) in &mut formats {
		format
			.set_arguments(&arg_matches)
			.map_err(|e| EachError::Usage {
				message: format!("Invalid argument for format {}: {}", format_id, e),
			})?;
	}

	let max_procs = if let Some(max_procs_str) = arg_matches.value_of("max-procs") {
		max_procs_str
			.parse::<usize>()
			.map_err(|e| EachError::Usage {
				message: format!("Invalid max-procs: {} ({})", &max_procs_str, e),
			})?
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
			let ext = path.extension().map(|ext| ext.to_string_lossy().to_string());
			let reader = Box::new(FileReader::new(&input_path).map_err(|e| EachError::Data {
				message: format!("Couldn't open file {}: {}", &input_path, e),
			})?);

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
			Some(format_id) => formats.get(format_id).ok_or_else(|| EachError::Usage {
				message: format!("Unknown format: {}", &format_id),
			})?,
			None => {
				formats::guess_format(ext, reader, &formats).ok_or_else(|| EachError::Data {
					message: "Unable to guess format for input".to_string(),
				})?
			}
		};

		let mut values = format.parse(reader).map_err(|e| EachError::Data {
			message: format!("failed to parse input: {}", e),
		})?;

		if let Some(query_str) = arg_matches.value_of("query") {
			let query = jmespath::compile(query_str).map_err(|e| EachError::Usage {
				message: format!("Invalid JMES query: {}", e),
			})?;

			let query_result = query.search(values).map_err(|e| EachError::Data {
				message: format!("Error evaluating JMES query: {}", e),
			})?;

			values = serde_json::to_value(query_result).map_err(|e| EachError::Data {
				message: format!("Error converting query result to JSON value: {}", e),
			})?;
		}

		let vec_values = values.as_array().ok_or_else(|| EachError::Data {
			message: "Input values are not an array".to_string(),
		})?;

		match action {
			Some(ref action) => process(vec_values, action)?,
			None => output_values.extend_from_slice(vec_values),
		}
	}

	if action.is_none() {
		let format = match arg_matches.value_of("output-format") {
			Some(format_id) => formats.get(format_id).ok_or_else(|| EachError::Usage {
				message: format!("Unknown output format: {}", &format_id),
			})?,
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

fn process(values: &[serde_json::Value], action: &Action) -> Result<(), EachError> {
	let results: Result<Vec<()>, EachError> = values
		.par_iter()
		.map(|value| -> Result<(), EachError> {
			let cmd = action.prepare(value).map_err(|e| EachError::Data {
				message: format!("failed to prepare command: {:?}", e),
			})?;

			let run = if action.prompt {
				let prompt = action.prompt(&cmd, value).map_err(|e| EachError::Data {
					message: format!("failed to render stdin: {:?}", e),
				})?;

				Confirm::new().with_prompt(&prompt).interact()?
			} else {
				true
			};

			if run {
				action.run(cmd).map_err(|e| EachError::Data {
					message: format!("failed to run command: {:?}", e),
				})?;
			}

			Ok(())
		})
		.collect();

	results.map(|_| ())
}
