#![feature(mpsc_select)]

extern crate crossbeam;
extern crate plaintalk;

mod err;

use std::convert;
use std::io::{self, BufReader, BufWriter};
use std::io::prelude::*;
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

fn connection<R: Read>(read: R, tx: Sender<ProtocolEvent>) -> Result<(), Error> {
	let mut parser = pullparser::PullParser::new(BufReader::new(read));

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
	let mut subprocess = process::Command::new("ssh")
		.args(&["trau.me", "nc -U /var/run/chattalk/socket"])
		.stdin(process::Stdio::piped())
		.stdout(process::Stdio::piped())
		.stderr(process::Stdio::null())
		.spawn().unwrap();

	let read = subprocess.stdout.as_mut().unwrap().by_ref();
	let write = subprocess.stdin.as_mut().unwrap().by_ref();
	let mut generator = pushgenerator::PushGenerator::new(BufWriter::new(write));
	generator.write_message(&[b"-", b"auth", b"unix"]).unwrap();

	let result = crossbeam::scope(move |scope| {
		let (tx_net, rx_net) = mpsc::channel();
		let subprocess_thread = scope.spawn(move || {
			connection(read, tx_net).unwrap();
// 			subprocess.wait().unwrap().code()
			Some(1)
		});

		let (tx_in, rx_in) = mpsc::channel();
		let input = scope.spawn(move || {
			let stdin = io::stdin();
			for line in stdin.lock().lines() {
				tx_in.send(line.unwrap()).unwrap();
			}
		});

		loop {
			select! {
				msg = rx_net.recv() => match msg {
					Ok(ProtocolEvent::Authenticated(id)) => {
						println!("Authenticated as {}", id);
					},
					Ok(ProtocolEvent::Shout(id, msg)) => {
						println!("{: >10}: {}", id, msg);
					},
					Err(_) => break
				},
				line = rx_in.recv() => match line {
					Ok(line) => generator.write_message(&[b"!", b"shout", &line.into_bytes()]).unwrap(),
					Err(_) => break
				}
			}
		}

		println!("Broke out of core loop (maybe hit ^C?)");

		let _ = input.join();
		subprocess_thread.join()
	});

	process::exit(result.unwrap_or(1))
}
