use failure::Error;
use handlebars::Handlebars;
use std::io::prelude::*;
use subprocess::Exec;

pub struct Action<'a> {
	command: String,
	args: Vec<String>,
	stdin: bool,
	pub prompt: bool,
	pub prompt_stdin: bool,
	templates: Handlebars<'a>,
}

impl<'a> Action<'a> {
	pub fn new(
		command: String,
		stdin: Option<String>,
		args: Vec<String>,
		prompt: bool,
		prompt_stdin: bool,
	) -> Result<Action<'a>, Error> {
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
			command,
			args: args?,
			stdin: stdin.is_some(),
			prompt,
			prompt_stdin,
			templates,
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
