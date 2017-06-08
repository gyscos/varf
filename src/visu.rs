use arff;
use arff::Population;
use hbs::{Template, HandlebarsEngine, DirectorySource};
use iron::Handler;
use iron::prelude::*;
use iron::status;
use mount::Mount;
// use serde_json;
use router::Router;
use staticfile::Static;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::mem::transmute;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use urlencoded::UrlEncodedQuery;

fn read_value<T: FromStr>(s: &str) -> Result<T, String>
    where T::Err: Display
{
    match T::from_str(s) {
        Ok(value) => Ok(value),
        Err(e) => Err(format!("could not read value: {}", e)),
    }
}

fn read_id(s: &str, content: &arff::ArffContent) -> Result<usize, String> {
    let id: usize = try!(read_value(s));
    if id >= content.attributes.len() {
        Err(format!("Invalid attribute id! {} > {}",
                    id,
                    content.attributes.len() - 1))
    } else {
        Ok(id)
    }
}


fn decorate(slices: Vec<Population>, min: f32, width: f32, i: usize) -> Range {
    let nmin = min + width * i as f32;
    let nmax = min + width * (i + 1) as f32;
    Range::new(format!("{}", (nmin + nmax) / 2.0), slices)
}

fn read_or<T: FromStr>(map: &HashMap<String, Vec<String>>, key: &str,
                       default: T)
                       -> Result<T, String>
    where T::Err: Display
{

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
    let mut divs: Vec<_> =
        (1..).take_while(|k| k * k <= n).filter(|k| n % k == 0).collect();

    // Then get all the ones above
    divs.iter()
        .rev()
        .map(|k| n / k)
        .collect::<Vec<usize>>()
        .iter()
        .map(|k| divs.push(*k))
        .collect::<Vec<()>>();

    divs
}

fn dist(k: usize, n: usize) -> usize {
    if k > n { k - n } else { n - k }
}

fn round_to_divider(value: usize, target: f32) -> usize {
    let delta = target as usize;
    if delta == 0 || target - delta as f32 > 0.0001 {
        return value;
    }

    // Get the list of the dividers of delta
    let mut divs = dividers(delta);
    divs.sort_by(|a, b| dist(*a, value).cmp(&dist(*b, value)));
    // Pick one close enough
    let closest = *divs.first().unwrap();
    // println!("Closest divs of {} from {}: {}", delta, value, closest);

    if dist(closest, value) < value / 3 {
        closest + 1
    } else {
        value
    }
}

#[derive(Serialize)]
struct PopViewData {
    lines: Vec<String>,
    class_description: String,
    description: String,
}

fn prepare_pop_view_data(content: &arff::ArffContent, req: &mut Request)
                         -> Result<PopViewData, String> {
    let data: AttViewData = try!(prepare_att_view_data(content, req));
    let map = match req.get::<UrlEncodedQuery>() {
        Err(e) => return Err(format!("cannot get query parameters: {}", e)),
        Ok(map) => map,
    };

    let att_id = match map.get("att_id") {
        Some(ids) => {
            if ids.is_empty() {
                0
            } else {
                try!(read_id(&ids[0], content))
            }
        }
        None => 0,
    };
    let att_cmp = match map.get("att_cmp") {
        Some(ids) => {
            if ids.is_empty() {
                0
            } else {
                try!(read_id(&ids[0], content))
            }
        }
        None => content.attributes.len() - 1,
    };

    let attr = &content.attributes[att_id];
    let cmp = &content.attributes[att_cmp];

    let slice_id = match map.get("slice") {
        Some(slice) => {
            if slice.is_empty() {
                return Err("empty slice parameter".to_string());
            } else {
                try!(read_id(&slice[0], content))
            }
        }
        None => return Err("no slice parameter".to_string()),
    };

    let (class, class_id) = match map.get("class") {
        Some(class) => {
            match class.first() {
                Some(class) => {
                    match content.get_class_id(att_cmp, class) {
                        Some(class_id) => (class, class_id),
                        None => {
                            return Err(format!("could not find class {}",
                                               class))
                        }
                    }
                }
                None => return Err("class parameter empty".to_string()),
            }
        }
        None => return Err("no class parameter".to_string()),
    };


    Ok(PopViewData {
           class_description: format!("{} = {}", cmp.name, class),
           description: format!("{} ~ {}",
                                attr.name,
                                data.samples[slice_id].label),
           lines: data.samples[slice_id].slices[class_id]
               .0
               .iter()
               .map(|&sample| content.describe_sample(sample))
               .collect(),
       })
}


#[derive(Serialize)]
struct AttViewData {
    title: String,
    name: String,
    filename: String,
    att_id: usize,
    classes: Vec<String>,
    attributes: Vec<String>,
    samples: Vec<Range>,
    numeric: bool,

    min: Option<f32>,
    max: Option<f32>,
    precision: Option<usize>,
}

fn prepare_att_view_data(content: &arff::ArffContent, req: &mut Request)
                         -> Result<AttViewData, String> {

    let ueq = req.get::<UrlEncodedQuery>();
    let hashmap = match ueq {
        Ok(map) => map,
        Err(_) => HashMap::new(),
    };

    // Default to the first attribute
    let att_id = match hashmap.get("att_id") {
        Some(ids) => {
            if ids.is_empty() {
                0
            } else {
                try!(read_id(&ids[0], content))
            }
        }
        None => 0,
    };

    // By default, compares to the last attribute (usually the class)
    let att_cmp = match hashmap.get("att_cmp") {
        Some(ids) => {
            if ids.is_empty() {
                0
            } else {
                try!(read_id(&ids[0], content))
            }
        }
        None => content.attributes.len() - 1,
    };

    let attr = &content.attributes[att_id];
    let cmp = &content.attributes[att_cmp];

    let class_tokens = match cmp.att_type.tokens() {
        Some(tokens) => tokens.to_owned(),
        None => {
            return Err(format!("Comparison to numeric attributes ({}) not supported",
                               cmp.name))
        }
    };

    let mut numeric = false;
    let mut min = None;
    let mut max = None;
    let mut precision = None;

    let ranges: Vec<Range> = match content.samples[att_id] {
        arff::AttributeSamples::Numeric(ref samples) => {
            // Numeric attribute. Ranges depend on precision, etc.
            numeric = true;

            if samples.is_empty() {
                Vec::new()
            } else {
                min = Some(try!(read_or(&hashmap, "min", samples[0].0)));
                max = Some(try!(read_or(&hashmap,
                                        "max",
                                        samples[samples.len() - 1].0)));

                let span = max.unwrap() - min.unwrap();
                // round n_slices to a divider of span, if it is a int
                precision = Some(try!(read_or(&hashmap, "precision", 26)));

                // Move a bit the precision if it can make things prettier
                let n_slices = round_to_divider(precision.unwrap(), span);

                let width = span / (n_slices - 1) as f32;
                // println!("max:{} min:{} width:{}", max, min, width);

                // Slice by value
                // Then group by class
                rangify(samples,
                        min.unwrap() - width / 2.0,
                        max.unwrap() + width / 2.0,
                        n_slices)
                        .iter()
                        .map(|pop| {
                            slice(pop,
                                  |i| {
                                      content.data[i].values[att_cmp]
                                          .text()
                                          .expect("value is not text!")
                                  },
                                  cmp.att_type
                                      .tokens()
                                      .expect("attribute is not text!")
                                      .len())
                        })
                        .enumerate()
                        .map(|(i, slices)| {
                                 decorate(slices, min.unwrap(), width, i)
                             })
                        .collect()
            }
        }
        arff::AttributeSamples::Text(ref groups) => {
            // Nominal attribute. Simple, one range per attribute value
            groups.iter()
                .map(|pop| {
                    slice(pop,
                          |i| content.data[i].values[att_cmp].text().expect("value is not text!!"),
                          cmp.att_type
                              .tokens()
                              .unwrap()
                              .len())
                })
                .enumerate()
                .map(|(i, slices)| {
                         Range::new(format!("{}", attr.att_type.tokens().unwrap()[i]), slices)
                     })
                .collect()
        }
        _ => Vec::new(),
    };

    Ok(AttViewData {
           title: content.title.clone(),
           name: attr.name.clone(),
           filename: content.filename.clone(),
           att_id: att_id,
           classes: class_tokens,
           attributes: content.attributes
               .iter()
               .map(|attr| attr.name.clone())
               .collect(),
           samples: ranges,
           numeric,
           min,
           max,
           precision,
       })
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
            Err(err) => {
                Ok(Response::with((status::Ok, format!("Error: {}", err))))
            }
            Ok(data) => {
                let mut resp = Response::new();

                // println!("{}", serde_json::to_string(&data).unwrap());
                resp.set_mut(Template::new("pop", data)).set_mut(status::Ok);
                Ok(resp)
            }
        }
    }
}

impl Handler for AttributeViewHandler {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {

        let data = prepare_att_view_data(self.content, req);
        match data {
            Err(err) => {
                Ok(Response::with((status::Ok, format!("Error: {}", err))))
            }
            Ok(data) => {
                let mut resp = Response::new();

                // println!("{}", serde_json::to_string(&data).unwrap());
                resp.set_mut(Template::new("visu", data)).set_mut(status::Ok);
                Ok(resp)
            }
        }
    }
}

pub fn serve_result<'a>(datadir: &'a str, port: u16,
                        content: &'a arff::ArffContent, open_browser: bool) {
    // Find the resource basedir
    println!("Loading templates from {}", datadir);

    let mut router = Router::new();

    router.get("/",
               AttributeViewHandler {
                   content: unsafe { transmute(content) },
               },
               "index");
    router.get("/pop",
               PopViewHandler { content: unsafe { transmute(content) } },
               "population");

    let mut mount = Mount::new();

    mount.mount("/", router).mount("/static/",
                                   Static::new(Path::new(&format!("{}/static",
                                                                  datadir))));

    // Load templates from there.
    let mut chain = Chain::new(mount);


    let mut hbse = HandlebarsEngine::new();
    hbse.add(Box::new(DirectorySource::new(&format!("{}/templates/",
                                                    datadir),
                                           ".html")));

    // load templates from all registered sources
    if let Err(r) = hbse.reload() {
        panic!("{}", r.description());
    }



    chain.link_after(hbse);
    println!("Now listening on port {}", port);

    if open_browser {
        Command::new("xdg-open")
            .arg(&format!("http://localhost:{}", port))
            .status()
            .ok()
            .expect("Could not open page in browser.");
    }

    Iron::new(chain).http(("0.0.0.0", port)).unwrap();
}


#[derive(Serialize, Deserialize)]
struct Range {
    label: String,
    slices: Vec<Population>,
    slices_len: Vec<usize>,
}

impl Range {
    fn new(label: String, slices: Vec<Population>) -> Self {
        let slices_len = slices.iter().map(|pop| pop.0.len()).collect();
        Range {
            label: label,
            slices: slices,
            slices_len: slices_len,
        }
    }
}

// Slice a population by the given function
fn slice<F>(pop: &Population, f: F, n_slices: usize) -> Vec<Population>
    where F: Fn(usize) -> usize
{


    let mut slices = Vec::with_capacity(n_slices);
    for _ in 0..n_slices {
        slices.push(Population(Vec::new()));
    }

    for i in pop.0.iter() {
        slices[f(*i)].0.push(*i);
    }

    slices
}

/// Map (f32,usize) by f32 to populations (chunks of usize)
fn rangify(data: &[(f32, usize)], min: f32, max: f32, slices: usize)
           -> Vec<Population> {
    if min == max {
        let mut result = Vec::new();
        let list: Vec<usize> = data.iter()
            .skip_while(|&&(f, _)| f < min)
            .take_while(|&&(f, _)| f == max)
            .map(|&(_, i)| i)
            .collect();
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
    for (k, i) in data.iter()
            .skip_while(|&&(f, _)| f < min)
            .take_while(|&&(f, _)| f < max)
            .map(|&(f, i)| (((f - min) / width) as usize, i)) {
        // And send it here
        result[k].0.push(i)
    }

    result
}
