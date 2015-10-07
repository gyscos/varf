use std::f32;
use std::fs;
use std::path;
use std::io;
use std::io::BufRead;
use std::str::FromStr;
use std::cmp::Ordering;

pub struct Population(pub Vec<usize>);

pub enum AttributeSamples {
    Numeric(Vec<(f32,usize)>),
    Text(Vec<Population>),
    BadType,
}

impl AttributeSamples {
    fn from_attr(attr: &Attribute) -> Self {
        match attr.att_type {
            AttributeType::Numeric => AttributeSamples::Numeric(Vec::new()),
            AttributeType::Text(ref tokens) => {
                let mut list = Vec::with_capacity(tokens.len());
                for _ in 0..tokens.len() {
                    list.push(Population(Vec::new()));
                }
                AttributeSamples::Text(list)
            },
            _ => AttributeSamples::BadType,
        }
    }
}

pub struct Instance {
    pub values: Vec<Value>,
}

pub enum Value {
    Numeric(f32),
    Text(usize),
    String(String),
    Missing,
}

impl Value {
    pub fn num(&self) -> Option<f32> {
        match self {
            &Value::Numeric(f) => Some(f),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<usize> {
        match self {
            &Value::Text(i) => Some(i),
            _ => None,
        }
    }

    pub fn string(&self) -> Option<&str> {
        match self {
            &Value::String(ref s) => Some(s),
            _ => None,
        }
    }
}

pub enum AttributeType {
    Numeric,
    Text(Vec<String>),
    String,
    Unknown,
}

impl AttributeType {
    // Parse an attribute type from the arff header
    fn parse(s: &str) -> Self {
        if s == "numeric" { return AttributeType::Numeric; }
        if s == "string" { return AttributeType::String; }
        if s.len() < 2 { println!("Bad type: {}", s); return AttributeType::Unknown; }

        let mut chars = s.chars();
        if chars.next().unwrap() != '{' || chars.last().unwrap() != '}' {
            panic!("Bad type: {}", s);
        }

        let tokens = {
            let len = s.len();
            s[1..len-1].split(',').map(|s| s.to_string()).collect()
        };
        AttributeType::Text(tokens)
    }

    /// If the type is numeric, returns the list of tokens.
    /// Returns None otherwise.
    pub fn tokens(&self) -> Option<&[String]> {
        match self {
            &AttributeType::Text(ref tokens) => Some(tokens),
            _ => None,
        }
    }
}

pub struct Attribute {
    pub name: String,
    pub att_type: AttributeType,
}

pub struct ArffContent {
    pub filename: String,
    pub title: String,

    // List of all data points
    pub data: Vec<Instance>,
    // List of attributes from the header
    pub attributes: Vec<Attribute>,
    // Per-attribute list of samples
    pub samples: Vec<AttributeSamples>,
}

fn parse_f32(s: &str) -> f32 {
    if s == "Infinity" {
        f32::INFINITY
    } else if s == "-Infinity" {
        f32::NEG_INFINITY
    } else {
        f32::from_str(s).ok().expect(&format!("Reading {}", s))
    }
}

impl ArffContent {

    pub fn get_class_id(&self, attribute: usize, class: &str) -> Option<usize> {
        self.attributes[attribute].att_type.tokens()
            .and_then(|tokens| tokens.iter().enumerate().find(|&(_,value)| value == class))
            .map(|(i,_)| i)
    }

    pub fn describe_sample(&self, sample_id: usize) -> String {
        let mut line = String::new();

        for (value, attr) in self.data[sample_id].values.iter().zip(self.attributes.iter()) {
            if attr.name == "id.ignore" {
                if let Some(s) = value.string() {
                    return s.to_string();
                }
            }
            match value {
                &Value::Numeric(f) => line.push_str(&format!("{}", f)),
                &Value::Text(i) => line.push_str(&attr.att_type.tokens().unwrap()[i]),
                &Value::String(ref s) => line.push_str(s),
                &Value::Missing => line.push('?'),
            };
            line.push(',');
        }

        line
    }

    fn load_data_line(&mut self, line: &str) {
        let values = line.split(',').zip(self.attributes.iter()).map(|(token, attr)| {
            if token == "?" {
                Value::Missing
            } else {
                match attr.att_type {
                    AttributeType::Numeric =>
                        Value::Numeric(parse_f32(token)),
                    AttributeType::Text(ref tokens) =>
                        Value::Text(tokens.iter().position(|s| s == token).unwrap()),
                    AttributeType::String =>
                        Value::String(token.to_string()),
                    _ => Value::Missing,
                }
            }
        }).collect();
        self.data.push(Instance {
            values: values,
        });
    }

    fn load_line(&mut self, line: &str) -> bool {
        let mut tokens = line.split(' ');
        match tokens.next() {
            Some("@relation") => self.title = tokens.next().unwrap().to_string(),
            Some("@attribute") => {
                let name = tokens.next().unwrap();
                let t = tokens.next().unwrap();
                let attr = Attribute{
                    name: name.to_string(),
                    att_type: AttributeType::parse(t),
                };
                self.samples.push(AttributeSamples::from_attr(&attr));
                self.attributes.push(attr);
            },
            Some("@data") => {
                // Consume the rest of the lines
                return true;
            },
            _ => (),
        }
        false
    }

    fn make_samples(&mut self) {
        for (id,instance) in self.data.iter().enumerate() {
            for (value, samples) in instance.values.iter().zip(self.samples.iter_mut()) {
                match samples {
                    &mut AttributeSamples::Numeric(ref mut list) => match value.num() {
                        Some(f) => list.push((f,id)),
                        None => (),
                    },
                    &mut AttributeSamples::Text(ref mut list) => match value.text() {
                        Some(i) => list[i].0.push(id),
                        None => (),
                    },
                    &mut AttributeSamples::BadType => (),
                }
            }
        }

        // Now sort it
        for samples in self.samples.iter_mut() {
            match samples {
                &mut AttributeSamples::Numeric(ref mut list) =>
                    list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal)),
                _ => (),
            }
        }
    }

    /// Loads a arff file
    pub fn new(filename: &path::Path) -> ArffContent {
        // Read the file line by line
        let file = match fs::File::open(filename) {
            Err(why) => panic!("Could not open file {}: {}", filename.display(), why),
            Ok(file) => file,
        };

        let mut content = ArffContent{
            filename: filename.to_str().unwrap().to_string(),
            title: String::new(),
            attributes: Vec::new(),
            data: Vec::new(),
            samples: Vec::new(),
        };

        let reader = io::BufReader::new(file);
        let mut reading_data = false;

        println!("Loading arff file...");
        for raw_line in reader.lines() {
            let line = raw_line.unwrap();
            if line.starts_with("%") { continue; }

            if reading_data {
                // We are loading the data!
                content.load_data_line(&line);
            } else {
                // We are still loading the header
                if content.load_line(&line) { reading_data = true; }
            }
        }

        content.make_samples();

        content
    }
}
