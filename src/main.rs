extern crate crossbeam;
extern crate plaintalk;

mod err;

use std::convert;
use std::io::{Read, Write, BufReader, BufWriter};
use std::process;
use std::sync::mpsc::{self, Sender, SendError};
use plaintalk::{pullparser, pushgenerator};

err!{ Error;
	PlainTalk(&'static str),
	PullParser(pullparser::Error),
	PushGenerator(pushgenerator::Error),
	ExpectationFailed(()),
	Sender(SendError<ProtocolEvent>)
}

enum ProtocolEvent {
	Authenticated(String),
	Shout(String, String),
}

fn expect<T, E>(field: Result<Option<T>, E>) -> Result<T, Error>
	where Error : convert::From<E>
{
	try!(field).ok_or(Error::ExpectationFailed(()))
}

fn expect_end(message: &pullparser::Message) -> Result<(), Error> {
	match message.at_end() {
		true => Ok(()),
		false => Err(Error::ExpectationFailed(()))
	}
}

fn expect_field(message: &mut pullparser::Message, expected: &'static [u8]) -> Result<(), Error> {
	let mut buf = vec![0u8; expected.len()];
	match try!(message.read_field(&mut buf)) {
		Some(len)
			if len == expected.len()
			=> if buf == expected { Ok(()) } else { Err(Error::ExpectationFailed(())) },
		Some(_) => Err(Error::ExpectationFailed(())),
		None => Err(Error::ExpectationFailed(())),
	}
}

fn connection<R: Read, W: Write>(read: R, write: W, tx: Sender<ProtocolEvent>) -> Result<(), Error> {
	let mut parser = pullparser::PullParser::new(BufReader::new(read));
	let mut generator = pushgenerator::PushGenerator::new(BufWriter::new(write));

	try!(generator.write_message(&[b"-", b"auth", b"unix"]));

	let mut msg_id_buf = [0u8; 10];
	let mut event_buf = [0u8; 10];
	while let Some(mut message) = try!(parser.get_message()) {
		let msg_id = try!{message.read_field_as_slice(&mut msg_id_buf)}
			.expect("PlainTalk parser yielded a message with zero fields");

		if msg_id == b"" && message.at_end() { break }

		if msg_id == b"*" {
			match try!{expect(message.read_field_as_slice(&mut event_buf))} {
				b"shout" => {
					let id = try!{expect(message.read_field_as_string())};
					let msg = try!{expect(message.read_field_as_string())};
					try!{expect_end(&mut message)};
					try!{tx.send(ProtocolEvent::Shout(id, msg))};
				}
				_ => try!{message.ignore_rest()}
			}
		} else if msg_id == b"-" {
			try!{expect_field(&mut message, b"ok")};
			let id = try!{expect(message.read_field_as_string())};
			try!{tx.send(ProtocolEvent::Authenticated(id))};
			try!{message.ignore_rest()};
		} else {
			try!{message.ignore_rest()};
		}
	}

	Ok(())
}

fn main() {
	let (tx, rx) = mpsc::channel();

	let result = crossbeam::scope(move |scope| {
		let subprocess = scope.spawn(move || {
			let mut subprocess = process::Command::new("ssh")
				.args(&["trau.me", "nc -U /var/run/chattalk/socket"])
				.stdin(process::Stdio::piped())
				.stdout(process::Stdio::piped())
				.stderr(process::Stdio::null())
				.spawn().unwrap();

			connection(
				subprocess.stdout.as_mut().unwrap().by_ref(),
				subprocess.stdin.as_mut().unwrap().by_ref(),
				tx
			).unwrap();

			subprocess.wait().unwrap().code()
		});

		while let Ok(message) = rx.recv() {
			match message {
				ProtocolEvent::Authenticated(id) => {
					println!("Authenticated as {}", id);
				},
				ProtocolEvent::Shout(id, msg) => {
					println!("{: >10}: {}", id, msg);
				},
			}
		}

		subprocess.join()
	});

	process::exit(result.unwrap_or(1))
}
