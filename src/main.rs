extern crate getopts;
extern crate iron;
extern crate router;
extern crate handlebars_iron as hbs;
extern crate rustc_serialize;
extern crate urlencoded;

mod arff;
mod visu;

use getopts::Options;
use std::env;
use std::path;
use std::str::FromStr;

fn read_params() -> (String, u16) {
    let args: Vec<String> = env::args().collect();
    let mut opts = Options::new();
    opts.optopt("p", "", "Sets the port to listen to", "PORT");
    let mut matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => panic!("Error: {}", f),
    };
    if matches.free.is_empty() {
        panic!("Did not specify a filename.");
    }

    let filename = matches.free.remove(0);

    let port = match matches.opt_str("p") {
        None => 8080,
        Some(p_str) => u16::from_str(&p_str).unwrap(),
    };

    (filename, port)
}

fn main() {

    let (filename, port) = read_params();

    let content = arff::ArffContent::new(path::Path::new(&filename));

    visu::serve_result(port, &content);
}
