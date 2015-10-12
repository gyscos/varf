use rustc_serialize::json::{Json,ToJson};
use arff::Population;

/// Structure sent to the templating engine when rendering the main page
#[derive(ToJson,Debug)]
pub struct VisuData {
    pub title: String,
    pub name: String,
    pub filename: String,
    pub att_id: u64,
    pub classes: Vec<String>,
    pub attributes: Vec<String>,
    pub samples: Vec<Range>,

    pub numeric: Option<NumericData>,
}

#[derive(ToJson,Debug)]
pub struct PopData {
    pub class_description: String,
    pub description: String,
    pub lines: Vec<String>,
}


/// These fields are only filled when the attribute is numeric
#[derive(ToJson,Debug)]
pub struct NumericData {
    pub min: f32,
    pub max: f32,

    // Rather the number of samples, may need a rename?
    pub precision: u64,
}

#[derive(ToJson,Debug)]
pub struct Range {
    pub label: String,
    pub slices: RangeSlices,
}

#[derive(Debug)]
pub enum RangeSlices {
    // This is used when the class attribute is Text
    Text(Vec<Population>),
    // TODO: cross numeric view
    // Numeric,
}

impl RangeSlices {
    pub fn text(&self) -> Option<&Vec<Population>> {
        match self {
            &RangeSlices::Text(ref pop) => Some(pop),
        }
    }
}

impl ToJson for RangeSlices {
    fn to_json(&self) -> Json {
        match self {
            &RangeSlices::Text(ref pop_list) => {
                pop_list.to_json()
            },
        }
    }
}

impl ToJson for Population {
    fn to_json(&self) -> Json {
        self.0.to_json()
    }
}

pub fn decorate(slices: RangeSlices, min: f32, width: f32, i: usize) -> Range {
    let nmin = min+width*i as f32;
    let nmax = min+width*(i+1) as f32;
    Range {
        label: format!("{}", (nmin+nmax)/2.0),
        slices: slices,
    }
}

