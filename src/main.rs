// These are for the Iron web framework
extern crate iron;
extern crate router;
extern crate handlebars_iron as hbs;
extern crate urlencoded;

extern crate rustc_serialize;
extern crate xdg;
extern crate getopts;
extern crate toml;

mod arff;
mod visu;

use getopts::Options;
use std::env;
use std::fs::File;
use std::path;
use std::str::FromStr;
use std::io::Read;

fn get_default_port() -> u16 {
    let default = 8080;

    let path = match xdg::get_config_home() {
        Ok(path) => path,
        Err(_) => return default,
    };
    let mut file = match File::open(path.join("varfrc")) {
        Ok(file) => file,
        Err(_) => return default,
    };
    let mut content = String::new();
    match file.read_to_string(&mut content) {
        Err(_) => return default,
        Ok(_) => (),
    }

    let table = match toml::Parser::new(&content).parse() {
        Some(table) => table,
        None => return default,
    };

    let port = match table.get("port") {
        Some(&toml::Value::Integer(port)) => port as u16,
        _ => return default,
    };

    port
}

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
        None => get_default_port(),
        Some(p_str) => u16::from_str(&p_str).unwrap(),
    };

    (filename, port)
}

fn main() {

    let (filename, port) = read_params();

    let content = arff::ArffContent::new(path::Path::new(&filename));

    visu::serve_result(port, &content);
}
