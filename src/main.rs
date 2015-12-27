extern crate plaintalk;

use std::convert;
use std::io::{Read, Write, BufReader, BufWriter};
use std::process;
use plaintalk::{pullparser, pushgenerator};

#[derive(Debug)]
enum Error {
	PlainTalk(&'static str),
	PullParser(pullparser::Error),
	PushGenerator(pushgenerator::Error),
}

impl convert::From<&'static str> for Error {
	fn from(err: &'static str) -> Error {
		Error::PlainTalk(err)
	}
}

impl convert::From<pullparser::Error> for Error {
	fn from(err: pullparser::Error) -> Error {
		Error::PullParser(err)
	}
}

impl convert::From<pushgenerator::Error> for Error {
	fn from(err: pushgenerator::Error) -> Error {
		Error::PushGenerator(err)
	}
}

fn connection<R: Read, W: Write>(read: R, write: W) -> Result<(), Error> {
	let mut parser = pullparser::PullParser::new(BufReader::new(read));
	let mut generator = pushgenerator::PushGenerator::new(BufWriter::new(write));

	try!(generator.write_message(&[b"-", b"auth", b"unix"]));

	while let Some(mut message) = try!(parser.get_message()) {
		try!(message.ignore_rest());
	}

	Ok(())
}

fn main() {
	let mut subprocess = process::Command::new("ssh")
		.args(&["trau.me", "nc -U /var/run/chattalk/socket"])
		.stdin(process::Stdio::piped())
		.stdout(process::Stdio::piped())
		.stderr(process::Stdio::null())
		.spawn().unwrap();

	connection(
		subprocess.stdout.as_mut().unwrap().by_ref(),
		subprocess.stdin.as_mut().unwrap().by_ref()
	).unwrap();

	process::exit(subprocess.wait().unwrap().code().unwrap_or(1))
}
