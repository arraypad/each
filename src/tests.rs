#[cfg(test)]
mod integration {
	extern crate assert_cli;
	extern crate lazy_static;

	use assert_cli::Assert;
	use std::collections::HashMap;

	const PEOPLE_CSV_PATH: &'static str = "test-resources/people.csv";
	const PEOPLE_TSV_PATH: &'static str = "test-resources/people.tsv";
	const PEOPLE_JSON_PATH: &'static str = "test-resources/people.json";

	fn norm_newlines<S: Into<String>>(s: S) -> String {
		return s.into().replace("\r\n", "\n");
	}

	fn read_resource(path: &'static str) -> String {
		std::fs::read_to_string(path).unwrap()
	}

	lazy_static::lazy_static! {
		static ref PEOPLE_CSV: String = read_resource(PEOPLE_CSV_PATH);
		static ref PEOPLE_TSV: String = read_resource(PEOPLE_TSV_PATH);
		static ref PEOPLE_JSON: String = read_resource(PEOPLE_JSON_PATH);
	}

	fn expect_people_json(got_str: &str) -> bool {
		let got_json: Vec<HashMap<String, String>> = serde_json::from_str(got_str).unwrap();
		let exp_json: Vec<HashMap<String, String>> =
			serde_json::from_str(PEOPLE_JSON.as_str()).unwrap();

		got_json == exp_json
	}

	#[test]
	fn csv_pipe_to_json() {
		Assert::main_binary()
			.stdin(PEOPLE_CSV.as_str())
			.succeeds()
			.and()
			.stdout()
			.satisfies(expect_people_json, "unexpected output")
			.unwrap();
	}

	#[test]
	fn csv_explicit_input_to_json() {
		Assert::main_binary()
			.with_args(&["-i", PEOPLE_CSV_PATH])
			.succeeds()
			.and()
			.stdout()
			.satisfies(expect_people_json, "unexpected output")
			.unwrap();
	}

	#[test]
	fn csv_explicit_input_format_to_json() {
		Assert::main_binary()
			.with_args(&["-i", PEOPLE_CSV_PATH, "-f", "csv"])
			.succeeds()
			.and()
			.stdout()
			.satisfies(expect_people_json, "unexpected output")
			.unwrap();
	}

	#[test]
	fn tsv_to_json() {
		Assert::main_binary()
			.with_args(&["-i", PEOPLE_TSV_PATH, "--csv-delimiter", "\t"])
			.succeeds()
			.and()
			.stdout()
			.satisfies(expect_people_json, "unexpected output")
			.unwrap();
	}

	#[test]
	fn json_pipe_to_json() {
		Assert::main_binary()
			.stdin(PEOPLE_JSON.as_str())
			.succeeds()
			.and()
			.stdout()
			.satisfies(expect_people_json, "unexpected output")
			.unwrap();
	}

	#[test]
	fn json_explicit_input_to_json() {
		Assert::main_binary()
			.with_args(&["-i", PEOPLE_JSON_PATH])
			.succeeds()
			.and()
			.stdout()
			.satisfies(expect_people_json, "unexpected output")
			.unwrap();
	}

	#[test]
	fn invalid_input() {
		Assert::main_binary()
			.with_args(&["-i", PEOPLE_JSON_PATH, "-f", "xxx"])
			.fails()
			.and()
			.stderr()
			.contains("error: 'xxx' isn't a valid value for '--format <FORMAT>'")
			.unwrap();
	}

	#[test]
	fn call_echo() {
		Assert::main_binary()
			.with_args(&["echo", "{{name}} <{{email}}>"])
			.stdin(PEOPLE_CSV.as_str())
			.succeeds()
			.and()
			.stdout()
			.satisfies(
				|s| {
					norm_newlines(s)
						== r#"Bart Simpson <bart@example.com>
Homer Simpson <homer@example.com>
"#
				},
				"unexpected output",
			)
			.unwrap();
	}

	#[test]
	fn json_to_csv() {
		Assert::main_binary()
			.with_args(&["-i", PEOPLE_JSON_PATH, "-F", "csv"])
			.succeeds()
			.and()
			.stdout()
			.satisfies(
				|s| norm_newlines(s) == norm_newlines(PEOPLE_CSV.as_str()),
				"unexpected output",
			)
			.unwrap();
	}

	#[test]
	fn csv_to_csv() {
		Assert::main_binary()
			.with_args(&["-i", PEOPLE_CSV_PATH, "-F", "csv"])
			.succeeds()
			.and()
			.stdout()
			.satisfies(
				|s| norm_newlines(s) == norm_newlines(PEOPLE_CSV.as_str()),
				"unexpected output",
			)
			.unwrap();
	}
}
