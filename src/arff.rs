use std::fs;
use std::path;
use std::io;
use std::io::BufRead;
use std::str::FromStr;
use std::cmp::Ordering;

pub enum AttributeSamples {
    Numeric(Vec<(f32,usize)>),
    Text(Vec<Vec<usize>>),
}

impl AttributeSamples {
    fn from_attr(attr: &Attribute) -> Self {
        match attr.att_type {
            AttributeType::Numeric => AttributeSamples::Numeric(Vec::new()),
            AttributeType::Text(ref tokens) => {
                let mut list = Vec::with_capacity(tokens.len());
                for _ in 0..tokens.len() {
                    list.push(Vec::new());
                }
                AttributeSamples::Text(list)
            },
        }
    }
}

pub struct Instance {
    pub values: Vec<Value>,
}

pub enum Value {
    Numeric(f32),
    Text(usize),
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
}

pub enum AttributeType {
    Numeric,
    Text(Vec<String>),
}

impl AttributeType {
    fn parse(s: &str) -> Self {
        if s == "numeric" { return AttributeType::Numeric; }
        if s.len() < 2 { panic!("Bad type: {}", s); }

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

    pub fn tokens(&self) -> &[String] {
        match self {
            &AttributeType::Text(ref tokens) => tokens,
            _ => panic!("Not a text attribute!"),
        }
    }
}

pub struct Attribute {
    pub name: String,
    pub att_type: AttributeType,
}

pub struct ArffContent {
    pub title: String,
    pub data: Vec<Instance>,
    pub attributes: Vec<Attribute>,
    pub samples: Vec<AttributeSamples>,
}

impl ArffContent {
    fn load_data_line(&mut self, line: &str) {
        let values = line.split(',').zip(self.attributes.iter()).map(|(token, attr)| {
            if token == "?" {
                Value::Missing
            } else {
                match attr.att_type {
                    AttributeType::Numeric => Value::Numeric(f32::from_str(token).unwrap()),
                    AttributeType::Text(ref tokens) => Value::Text(tokens.iter().position(|s| s == token).unwrap()),
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
        println!("Now building sample maps");

        for (id,instance) in self.data.iter().enumerate() {
            for (value, samples) in instance.values.iter().zip(self.samples.iter_mut()) {
                match samples {
                    &mut AttributeSamples::Numeric(ref mut list) => match value.num() {
                        Some(f) => list.push((f,id)),
                        None => (),
                    },
                    &mut AttributeSamples::Text(ref mut list) => match value.text() {
                        Some(i) => list[i].push(id),
                        None => (),
                    },
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

    pub fn new(filename: &path::Path) -> ArffContent {
        // Read the file line by line
        let file = match fs::File::open(filename) {
            Err(why) => panic!("Could not open file {}: {}", filename.display(), why),
            Ok(file) => file,
        };

        let mut content = ArffContent{
            title: String::new(),
            attributes: Vec::new(),
            data: Vec::new(),
            samples: Vec::new(),
        };

        let reader = io::BufReader::new(file);
        let mut reading_data = false;

        println!("Loading file");
        for raw_line in reader.lines() {
            let line = raw_line.unwrap();
            if line.starts_with("%") { continue; }

            if reading_data {
                content.load_data_line(&line);
            } else {
                if content.load_line(&line) { reading_data = true; }
            }
        }

        content.make_samples();

        content
    }
}
