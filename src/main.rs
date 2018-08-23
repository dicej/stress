#![deny(warnings)]

#[macro_use]
extern crate log;
extern crate clap;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate hyper;
extern crate pretty_env_logger;
extern crate tokio;

use clap::{App, Arg, ArgMatches};
use failure::Error;
use futures::future::{join_all, loop_fn, ok, result, Future, Loop};
use hyper::Client;
use std::{io::{self, stdin, BufRead},
          sync::{Arc, Mutex},
          time::{Duration, SystemTime}};
use tokio::runtime::Runtime;

// TODO: there may be a library out there that does this for us
fn millis(n: Duration) -> u64 {
    return (n.as_secs() * 1_000) + (n.subsec_nanos() as u64 / 1_000_000);
}

fn run(matches: ArgMatches) -> Result<(), Error> {
    let stdin = stdin();
    let lines = stdin
        .lock()
        .lines()
        .collect::<Result<Vec<String>, io::Error>>()?;

    let url = matches.value_of("url").unwrap().to_string();
    let count = matches.value_of("count").unwrap().parse::<u32>()?;

    type F = Box<Future<Item = Loop<u32, u32>, Error = Error> + Send>;

    let client = Client::new();

    struct State {
        responses: u64,
        then: SystemTime,
    }

    let state = Arc::new(Mutex::new(State {
        responses: 0,
        then: SystemTime::now(),
    }));

    Runtime::new()?.block_on(join_all(lines.clone().into_iter().map(move |line| {
        loop_fn(0, {
            let url = url.clone();
            let client = client.clone();
            let state = state.clone();
            move |number| {
                if number < count {
                    let url = format!("{}{}", url, line);
                    let uri = url.parse();
                    Box::new(
                        result(uri)
                            .map_err(move |_| format_err!("invalid URL: {}", url))
                            .and_then({
                                let client = client.clone();
                                move |uri| client.get(uri).map_err(Error::from)
                            })
                            .map({
                                let state = state.clone();
                                move |_| {
                                    let mut state = state.lock().unwrap();
                                    let elapsed = millis(
                                        state.then.elapsed().unwrap_or(Duration::from_secs(0)),
                                    );

                                    state.responses += 1;

                                    if elapsed > 1000 {
                                        println!(
                                            "{} responses per second",
                                            (state.responses * 1000) / elapsed
                                        );
                                        state.responses = 0;
                                        state.then = SystemTime::now();
                                    }

                                    Loop::Continue(number + 1)
                                }
                            }),
                    ) as F
                } else {
                    Box::new(ok(Loop::Break(0)))
                }
            }
        })
    })))?;

    Ok(())
}

fn main() {
    pretty_env_logger::init();

    if let Err(e) = run(App::new(env!("CARGO_PKG_DESCRIPTION"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .arg(Arg::with_name("url").help("url to connect to"))
        .arg(Arg::with_name("count").help("number of times to send requests"))
        .get_matches())
    {
        error!("exit on error: {:?}", e)
    }
}
