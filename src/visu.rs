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
use std::fmt::Display;
use hbs::{Template,HandlebarsEngine};
use arff;
use arff::Population;

fn read_value<T: FromStr> (s: &str) -> Result<T,String>
    where T::Err : Display {
    match T::from_str(s) {
        Ok(value) => Ok(value),
        Err(e) => Err(format!("could not read value: {}", e)),
    }
}

fn read_id(s: &str, content: &arff::ArffContent) -> Result<usize,String> {
    let id: usize = try!(read_value(s));
    if id >= content.attributes.len() {
        Err(format!("Invalid attribute id! {} > {}", id, content.attributes.len()-1))
    } else {
        Ok(id)
    }
}

fn decorate(slices: RangeSlices, min: f32, width: f32, i: usize) -> Range {
    let nmin = min+width*i as f32;
    let nmax = min+width*(i+1) as f32;
    Range {
        label: format!("{}", (nmin+nmax)/2.0),
        slices: slices,
    }
}

fn read_or<T: FromStr>(map: &HashMap<String, Vec<String>>, key: &str, default:T) -> Result<T, String> 
    where T::Err : Display {

    match map.get(key) {
        None => Ok(default),
        Some(list) => {
            if list.is_empty() {
                Ok(default)
            } else {
                Ok(try!(read_value(&list[0])))
            }
        }
    }
}

fn dividers(n: usize) -> Vec<usize> {
    // First get all dividers under the square root
    let mut divs = (1..).take_while(|k| k*k <= n).filter(|k| n%k == 0).collect::<Vec<usize>>();

    // Then get all the ones above
    divs.iter().rev().map(|k| n/k).collect::<Vec<usize>>().iter().map(|k| divs.push(*k)).collect::<Vec<()>>();

    divs
}

fn dist(k: usize, n: usize) -> usize {
    if k > n { k - n }
    else { n - k }
}

fn round_to_divider(value: usize, target: f32) -> usize {
    let delta = target as usize;
    if delta == 0 || target - delta as f32 > 0.0001 {
        return value;
    }

    let divs = dividers(delta);
    // Pick one close enough
    let closest = *divs.iter().min_by(|k| dist(**k,value)).unwrap();
    // println!("Closest divs of {} from {}: {}", delta, value, closest);

    if dist(closest, value) < value/3 { closest+1 }
    else { value }
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

    if cmp.att_type.tokens() == None {
        return Err(format!("Comparison to numeric attributes ({}) not supported", cmp.name));
    }

    let mut map = BTreeMap::<String,Json>::new();
    map.insert("title".to_string(), content.title.to_json());
    map.insert("name".to_string(), attr.name.to_json());
    map.insert("filename".to_string(), content.filename.to_json());
    map.insert("att_id".to_string(), att_id.to_json());
    map.insert("attributes".to_string(), content.attributes.iter().map(|att| att.name.clone()).collect::<Vec<String>>().to_json());

    let ranges: Vec<Range> = match content.samples[att_id] {
        arff::AttributeSamples::Numeric(ref samples) => {
            map.insert("numeric".to_string(), true.to_json());

            if samples.is_empty() {
                Vec::new()
            } else {
                let mut min = try!(read_or(&hashmap, "min", samples[0].0));
                let mut max = try!(read_or(&hashmap, "max", samples[samples.len()-1].0));

                let span = max - min;
                // round n_slices to a divider of span, if it is a int
                let precision = try!(read_or(&hashmap, "precision", 51));
                map.insert("min".to_string(), min.to_json());
                map.insert("max".to_string(), max.to_json());
                map.insert("precision".to_string(), precision.to_json());

                let n_slices = round_to_divider(precision, span);

                let width = span / (n_slices-1) as f32;
                // println!("max:{} min:{} width:{}", max, min, width);
                max += width/2.0;
                min -= width/2.0;

                rangify(samples, min, max, n_slices).iter()
                    .map(|pop| slice(pop,
                                     |i| content.data[i].values[att_cmp].text().expect("value is not text!"),
                                     cmp.att_type.tokens().expect("attribute is not text!").len()))
                    .enumerate()
                    .map(|(i, slices)| decorate(slices, min, width, i))
                    .collect()
            }
        },
        arff::AttributeSamples::Text(ref groups) => {
            map.insert("numeric".to_string(), false.to_json());
            groups.iter().map(|pop| slice(pop,
                                   |i| content.data[i].values[att_cmp].text().expect("value is not text!!"),
                                   cmp.att_type.tokens().unwrap().len()))
                .enumerate()
                .map(|(i, slices)| Range{ label: format!("{}", attr.att_type.tokens().unwrap()[i]), slices: slices})
                .collect()
        },
        _ => Vec::new(),
    };
    map.insert("samples".to_string(), ranges.to_json());

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

pub fn serve_result<'a>(datadir: &'a str, port: u16, content: &'a arff::ArffContent) {
    // Find the resource basedir
    println!("Loading templates from {}", datadir);

    let mut router = Router::new();

    router.get("/", AttributeViewHandler{ content: unsafe { transmute(content) } });

    // Load templates from there.
    println!("Now listening on port {}", port);
    let mut chain = Chain::new(router);
    chain.link_after(HandlebarsEngine::new(&format!("{}/templates/", datadir), ".html"));
    Iron::new(chain).http(("0.0.0.0", port)).unwrap();
}


impl ToJson for Population {
    fn to_json(&self) -> Json {
        self.0.to_json()
    }
}

enum RangeSlices {
    Text(Vec<Population>),
    // TODO: cross numeric view
    // Numeric,
}

struct Range {
    label: String,
    slices: RangeSlices,
}

impl ToJson for Range {
    fn to_json(&self) -> Json {
        let mut map = BTreeMap::new();
        map.insert("label".to_string(), self.label.to_json());
        match self.slices {
            RangeSlices::Text(ref pop_list) => {
                map.insert("slices".to_string(), pop_list.to_json());
                map.insert("slices_len".to_string(), pop_list.iter().map(|p| p.0.len()).collect::<Vec<usize>>().to_json());
            },
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
