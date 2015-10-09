use std::io::Write;
use std::collections::BTreeMap;
use std::collections::HashMap;
use rustc_serialize::json::{Json,ToJson};
use urlencoded::UrlEncodedQuery;
use staticfile::Static;
use iron::prelude::*;
use iron::Handler;
use iron::status;
use mount::Mount;
use router::Router;
use std::str::FromStr;
use std::path::Path;
use std::mem::transmute;
use std::fmt::Display;
use handlebars::{Handlebars, RenderError, RenderContext, Helper, Context};
use hbs::{Template,HandlebarsEngine};
use arff;
use arff::Population;
use data;

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
    let mut divs: Vec<_> = (1..)
        .take_while(|k| k*k <= n)
        .filter(|k| n%k == 0)
        .collect();

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

    // Get the list of the dividers of delta
    let mut divs = dividers(delta);
    divs.sort_by(|a,b| dist(*a,value).cmp(&dist(*b,value)));
    // Pick one close enough
    let closest = *divs.first().unwrap();
    // println!("Closest divs of {} from {}: {}", delta, value, closest);

    if dist(closest, value) < value/3 { closest+1 }
    else { value }
}

fn prepare_pop_view_data(content: &arff::ArffContent, req: &mut Request)
    -> Result<Json,String>
{
    let data = try!(prepare_att_view_data(content, req));
    let map = match req.get::<UrlEncodedQuery>() {
        Err(e) => return Err(format!("cannot get query parameters: {}", e)),
        Ok(map) => map,
    };

    let att_id = match map.get("att_id") {
        Some(ids) => if ids.is_empty() { 0 } else { try!(read_id(&ids[0], content)) },
        None => 0,
    };
    let att_cmp = match map.get("att_cmp") {
        Some(ids) => if ids.is_empty() { 0 } else { try!(read_id(&ids[0], content)) },
        None => content.attributes.len() - 1,
    };

    let attr = &content.attributes[att_id];
    let cmp = &content.attributes[att_cmp];

    let slice_id = match map.get("slice") {
        Some(slice) =>
            if slice.is_empty() { return Err("empty slice parameter".to_string()); }
            else { try!(read_id(&slice[0], content)) },
        None => return Err("no slice parameter".to_string()),
    };

    let class = match map.get("class") {
        Some(class) => match class.first() {
            Some(class) => match content.get_class_id(att_cmp, class) {
                Some(class) => class,
                None => return Err(format!("could not find class {}", class)),
            },
            None => return Err("class parameter empty".to_string()),
        },
        None => return Err("no class parameter".to_string()),
    };

    let mut map = BTreeMap::<String,Json>::new();

    let lines: Vec<_> = data.samples[slice_id].slices.text().unwrap()
        [class].0.iter().map(|&sample| {
            // println!("Sample: {:?}", sample);
            content.describe_sample(sample)
        }).collect();

    map.insert("lines".to_string(), lines.to_json());

    map.insert("class_description".to_string(), format!("{} = {}", cmp.name, class).to_json());
    map.insert("description".to_string(), format!("{} ~ {}", attr.name, &data.samples[slice_id].label).to_json());

    Ok(Json::Object(map))
}

fn prepare_att_view_data(content: &arff::ArffContent, req: &mut Request)
    -> Result<data::VisuData,String>
{

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

    // By default, compares to the last attribute (usually the class)
    let att_cmp = match hashmap.get("att_cmp") {
        Some(ids) => if ids.is_empty() { 0 } else { try!(read_id(&ids[0], content)) },
        None => content.attributes.len() - 1,
    };

    let attr = &content.attributes[att_id];
    let cmp = &content.attributes[att_cmp];

    let class_tokens = match cmp.att_type.tokens() {
        Some(tokens) => tokens,
        None => return Err(format!("Comparison to numeric attributes ({}) not supported", cmp.name)),
    };


    let (ranges,numeric) = match content.samples[att_id] {
        arff::AttributeSamples::Numeric(ref samples) => {
            // Numeric attribute. Ranges depend on precision, etc.

            if samples.is_empty() {
                (Vec::new(),None)
            } else {
                let mut min = try!(read_or(&hashmap, "min", samples[0].0));
                let mut max = try!(read_or(&hashmap, "max", samples[samples.len()-1].0));

                let span = max - min;
                // round n_slices to a divider of span, if it is a int
                let precision = try!(read_or(&hashmap, "precision", 51));

                let numeric = data::NumericData {
                    min: min,
                    max: max,
                    precision: precision as u64,
                };

                // Move a bit the precision if it can make things prettier
                let n_slices = round_to_divider(precision, span);

                let width = span / (n_slices-1) as f32;
                // println!("max:{} min:{} width:{}", max, min, width);
                max += width/2.0;
                min -= width/2.0;

                // Slice by value
                // Then group by class
                let ranges: Vec<_> = rangify(samples, min, max, n_slices).iter()
                    .map(|pop| slice(pop,
                                     |i| content.data[i].values[att_cmp].text().expect("value is not text!"),
                                     cmp.att_type.tokens().expect("attribute is not text!").len()))
                    .enumerate()
                    .map(|(i, slices)| data::decorate(slices, min, width, i))
                    .collect();

                (ranges, Some(numeric))
            }
        },
        arff::AttributeSamples::Text(ref groups) => {
            // Nominal attribute. Simple, one range per attribute value
            let ranges: Vec<_> = groups.iter().map(|pop| slice(pop,
                                   |i| content.data[i].values[att_cmp].text().expect("value is not text!!"),
                                   cmp.att_type.tokens().unwrap().len()))
                .enumerate()
                .map(|(i, slices)| data::Range{ label: format!("{}", attr.att_type.tokens().unwrap()[i]), slices: slices})
                .collect();
            (ranges, None)
        },
        _ => (Vec::new(), None),
    };
    let data = data::VisuData {
        title: content.title.clone(),
        name: attr.name.clone(),
        filename: content.filename.clone(),
        att_id: att_id as u64,
        classes: class_tokens.to_vec(),
        attributes: content.attributes.iter().map(|att| att.name.clone()).collect(),
        samples: ranges,
        numeric: numeric,
    };

    Ok(data)
}

struct AttributeViewHandler {
    content: &'static arff::ArffContent,
}

struct PopViewHandler {
    content: &'static arff::ArffContent,
}

impl Handler for PopViewHandler {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
        let data = prepare_pop_view_data(self.content, req);
        match data {
            Err(err) => Ok(Response::with((status::Ok, format!("Error: {}", err)))),
            Ok(json) => {
                let mut resp = Response::new();

                resp.set_mut(Template::new("pop", json)).set_mut(status::Ok);
                Ok(resp)
            },
        }
    }
}

impl Handler for AttributeViewHandler {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {

        let data = prepare_att_view_data(self.content, req);
        match data {
            Err(err) => Ok(Response::with((status::Ok, format!("Error: {}", err)))),
            Ok(data) => {
                let mut resp = Response::new();

                resp.set_mut(Template::new("visu", data.to_json())).set_mut(status::Ok);
                Ok(resp)
            },
        }
    }
}

// Handlebars apparently doesn't support `.length` notation, so let's hack it.
fn setup_length_helper(handlebars: &mut Handlebars) {
    let f = Box::new(|c: &Context, h: &Helper, _: &Handlebars, rc: &mut RenderContext| -> Result<(), RenderError>{
        let value_param = try!(h.param(0).ok_or_else(|| RenderError{desc:"Param not found for helper \"length\""}));
        let value = c.navigate(rc.get_path(), &value_param);
        match *value {
            Json::Array(ref list) => {
                let r = format!("{}", list.len());
                try!(rc.writer.write(r.into_bytes().as_ref()));
                Ok(())
            }
            _ => Err(RenderError{desc:"Param is not an iteratable."}),
        }
    });
    handlebars.register_helper("length", f);
}

pub fn serve_result<'a>(datadir: &'a str, port: u16, content: &'a arff::ArffContent) {
    // Find the resource basedir
    println!("Loading templates from {}", datadir);

    let mut router = Router::new();

    router.get("/", AttributeViewHandler{ content: unsafe { transmute(content) } });
    router.get("/pop", PopViewHandler{ content: unsafe { transmute(content) } });

    let mut mount = Mount::new();

    mount
        .mount("/", router)
        .mount("/static/", Static::new(Path::new(&format!("{}/static", datadir))));

    // Load templates from there.
    println!("Now listening on port {}", port);
    let mut chain = Chain::new(mount);
    let hbe = HandlebarsEngine::new(&format!("{}/templates/", datadir), ".html");
    setup_length_helper(&mut *hbe.registry.write().unwrap());
    chain.link_after(hbe);
    Iron::new(chain).http(("0.0.0.0", port)).unwrap();
}


// Slice a population by the given function
fn slice<F>(pop: &Population, f: F, n_slices: usize) -> data::RangeSlices
    where F: Fn(usize) -> usize {


    let mut slices = Vec::with_capacity(n_slices);
    for _ in 0..n_slices { slices.push(Population(Vec::new())); }

    for i in pop.0.iter() {
        slices[f(*i)].0.push(*i);
    }

    data::RangeSlices::Text(slices)
}

/// Map (f32,usize) by f32 to populations (chunks of usize)
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

    // First, clamp to min & max
    // map to slice ID : f -> (f-min) / width
    for (k,i) in data.iter()
        .skip_while(|&&(f,_)| f < min)
        .take_while(|&&(f,_)| f < max)
        .map(|&(f,i)| (((f - min)/width) as usize, i) )
    {
        // And send it here
        result[k].0.push(i)
    }

    result
}
