# Each: build command lines from CSV, JSON etc.

Do you find yourself juggling awkward and brittle incantations of _grep_, _cut_ and _xargs_ when working with structured data? _Each_ aims to provide a better way of working with structured data on the command line.

* [Examples](#Examples)
* [Installation](#Installation)
* [Security considerations](#Security-considerations)
* [License (MIT)](#License)

## Examples

#### Convert CSV into JSON
When no command argument is supplied, _each_ pretty-prints the parsed list of items as JSON (or into another supported format using the `-F` argument).

Contents of `people.csv`:
```
name,email
Bart Simpson,bart@example.com
Homer Simpson,homer@example.com
```

Run _each_:
```sh
each < people.csv > people.json
```

The resulting `people.json`:
```json
[
	{
		"name": "Bart Simpson",
		"email": "bart@example.com"
	},
	{
		"name": "Homer Simpson",
		"email": "homer@example.com"
	}
]
```

#### Use named fields in command arguments

Each command argument is a Handlebars template with the full row of data available in its context.

```sh
each echo '{{name}} <{{email}}>' < people.csv
```

Output:
```
Bart Simpson <bart@example.com>
Homer Simpson <homer@example.com>

```

#### Ping a list of EC2 instances returned from the AWS CLI.

Since _each_ reads from stdin by default it plays nicely with other CLI tools including _jq_.

```sh
aws ec2 describe-instances | \
	jq '.Instances' | \
	each ping -c 1 {{PublicIpAddress}}
```

#### Supply stdin to each command

You can also pass a string using the `-s` / `--stdin` argument (or the contents of a file using the `-S` / `--stdin-file` argument) as a template to be sent to the stdin of each command process.

E.g. the _mail_ program reads the message from stdin:
```sh
each -i people.json \
	--stdin='Dear {{name}}, have a great day!' \
	mail -s 'Exciting message' {{email}}
```

#### Run commands in parallel

Like _xargs_ you can provide the `-P` / `--max-procs` argument to run many commands in parallel. This is particularly useful for long running but low resource-intensive commands:

```sh
each -i videos.json -P 16 -- youtube-dl {{url}}

```

#### Prompt for confirmation of each command

Also like xargs, the `-p` / `--interactive` flag will show the resulting command line and prompt you to confirm running each one. This gives you an opportunity to inspect the command before starting a potentially expensive / dangerous operation. Note that this doesn't show the interpolated value passed to stdin if you used the `--stdin` or `--stdin-file` arguments since it's often large. To include that in the prompt use `--prompt-stdin`.

```sh
each -p rm {{tmppath}} < datasets.csv
```

Outputs:
```sh
rm '/tmp/tmp.OFW3bJ5psl' [Y/n]
```

## Installation

```sh
cargo install each
```

## Security considerations

_Each_ executes commands on your behalf using potentially untrusted data, so please use it with the utmost care.

Be particularly wary of feeding it files which are writable by other users or which you have fetched from an untrusted source. It's possible a data file may have changed between the time you've inspected it and when you invoke _each_ with it, or an attacker to have crafted the file to make it appear valid.

While it doesn't apply template substitutions to the first command argument (the executable path) it's quite possible to get unexpected behaviour from subsequent arguments.

For example if your executable is a script interpreter then it may be perfectly valid to supply multiple scripts which might lead to untrusted code execution. The following invocation passes two data-controlled parameters into a Perl script in what might appear at first glance to be a safe manner: 

```sh
each -i people.json -- \
	perl -e 'print reverse @ARGV' '{{name}}' '{{email}}'
```

However the following evil user gets to run arbitrary Perl code (in this case calling _uptime_):
```json
{
	"name": "-e",
	"email": "; system uptime"
}
```

## License

Copyright 2019 Bobweb Ltd

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
