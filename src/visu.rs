use router::Router;
use std::collections::BTreeMap;
use std::collections::HashMap;
use rustc_serialize::json::{Json,ToJson};
use urlencoded::UrlEncodedQuery;
use iron::prelude::*;
use iron::Handler;
use iron::status;
use std::str::FromStr;
use std::mem::transmute;
use hbs::{Template,HandlebarsEngine};
use arff;

fn get_data_dir() -> &'static str {
    match option_env!("VARF_HOME") {
        Some(path) => path,
        None => "/usr/share/varf",
    }
}

fn read_id(s: &str, content: &arff::ArffContent) -> Result<usize,String> {
    match usize::from_str(s) {
        Err(e) => return Err(format!("could not read value: {}", e)),
        Ok(id) => if id >= content.attributes.len() {
            return Err(format!("Invalid attribute id! {} > {}", id, content.attributes.len()-1));
        } else {
            Ok(id)
        },
    }
}

fn decorate(slices: RangeSlices, min: f32, width: f32, i: usize) -> Range {
    Range {
        min: min+width*i as f32,
        max: min+width*(i+1) as f32,
        slices: slices,
    }
}

fn prepare_att_view_data(content: &arff::ArffContent, req: &mut Request) -> Result<Json,String> {

    let ueq = req.get::<UrlEncodedQuery>();
    let hashmap = match ueq {
        Ok(map) => map,
        Err(_) => HashMap::new(),
    };

    // Default to the first attribute
    let att_id = match hashmap.get("att_id") {
        Some(ids) => if ids.is_empty() { 0 } else { try!(read_id(&ids[0], content)) },
        None => 0,
    };

    // Default to the class
    let att_cmp = match hashmap.get("att_cmp") {
        Some(ids) => if ids.is_empty() { 0 } else { try!(read_id(&ids[0], content)) },
        None => content.attributes.len() - 1,
    };

    let attr = &content.attributes[att_id];
    let cmp = &content.attributes[att_cmp];

    let mut map = BTreeMap::<String,Json>::new();
    map.insert("title".to_string(), content.title.to_json());
    map.insert("name".to_string(), attr.name.to_json());
    map.insert("attributes".to_string(), content.attributes.iter().map(|att| att.name.clone()).collect::<Vec<String>>().to_json());

    match content.samples[att_id] {
        arff::AttributeSamples::Numeric(ref samples) => {

            let min = samples[0].0;
            let mut max = samples.iter().last().unwrap().0;
            let span = max - min;
            let mut n_slices = 49;
            let width = span / n_slices as f32;
            max += width;
            n_slices += 1;

            let ranges: Vec<Range> = rangify(samples, min, max, n_slices).iter()
                .map(|pop| slice(pop,
                                 |i| content.data[i].values[att_cmp].text().unwrap(),
                                 cmp.att_type.tokens().len()))
                .enumerate()
                .map(|(i, slices)| decorate(slices, min, width, i))
                .collect();

            map.insert("samples".to_string(), ranges.to_json());
        },
        arff::AttributeSamples::Text(_) => (),
    }

    Ok(Json::Object(map))
}

struct AttributeViewHandler {
    content: &'static arff::ArffContent,
}

impl Handler for AttributeViewHandler {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
        let data = prepare_att_view_data(self.content, req);
        match data {
            Err(err) => Ok(Response::with((status::Ok, format!("Error: {}", err)))),
            Ok(json) => {
                let mut resp = Response::new();

                resp.set_mut(Template::new("visu", json)).set_mut(status::Ok);
                Ok(resp)
            },
        }
    }
}

pub fn serve_result<'a>(port: u16, content: &'a arff::ArffContent) {
    // Find the resource basedir
    let datadir = get_data_dir();
    println!("Loading templates from {}", datadir);

    let mut router = Router::new();

    router.get("/", AttributeViewHandler{ content: unsafe { transmute(content) } });

    // Load templates from there.
    println!("Now listening on port {}", port);
    let mut chain = Chain::new(router);
    chain.link_after(HandlebarsEngine::new(&format!("{}/templates/", datadir), ".html"));
    Iron::new(chain).http(("localhost", port)).unwrap();
}

struct Population(Vec<usize>);

impl ToJson for Population {
    fn to_json(&self) -> Json {
        self.0.to_json()
    }
}

enum RangeSlices {
    Text(Vec<Population>),
    // TODO: cross numeric view
    Numeric,
}

struct Range {
    min: f32,
    max: f32,
    slices: RangeSlices,
}

impl ToJson for Range {
    fn to_json(&self) -> Json {
        let mut map = BTreeMap::new();
        map.insert("min".to_string(), self.min.to_json());
        map.insert("max".to_string(), self.max.to_json());
        match self.slices {
            RangeSlices::Text(ref pop_list) => { map.insert("slices".to_string(), pop_list.to_json()); },
            _ => (),
        }

        Json::Object(map)
    }
}

fn slice<F>(pop: &Population, f: F, n_slices: usize) -> RangeSlices
    where F: Fn(usize) -> usize {


    let mut slices = Vec::with_capacity(n_slices);
    for _ in 0..n_slices { slices.push(Population(Vec::new())); }

    pop.0.iter().map(|i| slices[f(*i)].0.push(*i) ).collect::<Vec<()>>();

    RangeSlices::Text(slices)
}

fn rangify(data: &[(f32,usize)], min: f32, max: f32, slices: usize) -> Vec<Population> {
    if min == max {
        let mut result = Vec::new();
        let list: Vec<usize> = data.iter()
            .skip_while(|&&(f,_)| f < min)
            .take_while(|&&(f,_)| f == max)
            .map(|&(_,i)| i).collect();
        result.push(Population(list));
        return result;
    }

    let mut result = Vec::with_capacity(slices);
    let width = (max - min) / slices as f32;
    for _ in 0..slices {
        result.push(Population(Vec::new()));
    }

    data.iter().skip_while(|&&(f,_)| f < min).take_while(|&&(f,_)| f < max)
        .map(|&(f,i)| (((f - min)/width) as usize, i) )
        .map(|(k,i)| result[k].0.push(i))
        .collect::<Vec<()>>();

    result
}
