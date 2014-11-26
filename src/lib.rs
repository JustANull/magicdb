#![crate_name = "magicdb"]
#![crate_type = "lib"]

#![feature(macro_rules)]

extern crate serialize;

use serialize::json;
use std::collections;
use std::io;
use std::rc;

#[deriving(Clone, PartialEq, Eq, Show)]
pub enum Card {
    Normal(rc::Rc<Box<CardInfo>>),
    Split(rc::Rc<Box<CardInfo>>, rc::Rc<Box<CardInfo>>),
    Flip(rc::Rc<Box<CardInfo>>, rc::Rc<Box<CardInfo>>),
    DoubleFaced(rc::Rc<Box<CardInfo>>, rc::Rc<Box<CardInfo>>)
}
#[deriving(Clone, PartialEq, Eq, Show)]
pub struct CardInfo {
    name:  String,
    mana:  Option<Vec<Mana>>,
    color: Option<Vec<String>>,

    supertypes: Option<Vec<String>>,
    types:      Option<Vec<String>>,
    subtypes:   Option<Vec<String>>,

    image_name:  String,
    text:        Option<String>,
    flavor_text: Option<String>,

    extra: ExtraInfo
}
#[deriving(Clone, PartialEq, Eq, Show)]
pub enum Color {
    White, Blue, Black, Red, Green
}
#[deriving(Clone, PartialEq, Eq, Show)]
pub enum ExtraInfo {
    None,
    PowerToughness(String, String),
    StartingLoyalty(i64)
}
#[deriving(Clone, PartialEq, Eq, Show)]
pub enum Mana {
    Colored(Color),
    // An amount of colorless
    Colorless(u32),
    // e.g. payable by red or green (Manamorphose)
    Hybrid(Color, Color),
    // e.g. payable by 2 colorless or white (Spectral Procession)
    ColorlessHybrid(u32, Color),
    // e.g. half a white (Little Girl)
    Half(Color)
}

#[deriving(Clone, PartialEq, Show)]
pub enum BuilderError {
    NoTopLevelObject,
    InvalidCardObject(String),
    NoCardField(String, &'static str),
    InvalidCardField(String, &'static str),
    Json(json::BuilderError)
}

macro_rules! read_optional(
    ($f:expr, $js:expr, $field:expr, $card_name:expr) => (
        match $f($js, $field, $card_name) {
            Ok(val)     => Ok(Some(val)),
            Err(reason) => match reason {
                BuilderError::NoCardField(_, _) => Ok(None),
                _                               => Err(reason)
            }
        }
    );
)

fn read_integer(js: &json::JsonObject, field: &'static str, card_name: &String) -> Result<i64, BuilderError> {
    match js.get(&field.to_string()) {
        Some(obj) => match obj.as_i64() {
            Some(i) => Ok(i),
            None    => Err(BuilderError::InvalidCardField(card_name.clone(), field))
        },
        None => Err(BuilderError::NoCardField(card_name.clone(), field))
    }
}
fn read_string(js: &json::JsonObject, field: &'static str, card_name: &String) -> Result<String, BuilderError> {
    match js.get(&field.to_string()) {
        Some(obj) => match obj.as_string() {
            Some(s) => Ok(s.to_string()),
            None    => Err(BuilderError::InvalidCardField(card_name.clone(), field))
        },
        None => Err(BuilderError::NoCardField(card_name.clone(), field))
    }
}
fn read_string_array(js: &json::JsonObject, field: &'static str, card_name: &String) -> Result<Vec<String>, BuilderError> {
    match js.get(&field.to_string()) {
        Some(obj) => match obj.as_array() {
            Some(arr) => {
                let mut s_vec = Vec::new();

                for arr_obj in arr.iter() {
                    match arr_obj.as_string() {
                        Some(s) => {
                            s_vec.push(s.to_string())
                        },
                        None => return Err(BuilderError::InvalidCardField(card_name.clone(), field))
                    }
                }

                Ok(s_vec)
            },
            None => Err(BuilderError::InvalidCardField(card_name.clone(), field))
        },
        None => Err(BuilderError::NoCardField(card_name.clone(), field))
    }
}

fn read_color(c: char, card_name: &String) -> Result<Color, BuilderError> {
    match c.to_uppercase() {
        'W' => Ok(Color::White),
        'U' => Ok(Color::Blue),
        'B' => Ok(Color::Black),
        'R' => Ok(Color::Red),
        'G' => Ok(Color::Green),
        _   => Err(BuilderError::InvalidCardField(card_name.clone(), "manaCost"))
    }
}
fn read_extra(js: &json::JsonObject, card_name: &String) -> Result<ExtraInfo, BuilderError> {
    match try!(read_optional!(read_string, js, "power", card_name)) {
        Some(power) => Ok(ExtraInfo::PowerToughness(power, try!(read_string(js, "toughness", card_name)))),
        None        => {
            match try!(read_optional!(read_integer, js, "loyalty", card_name)) {
                Some(loyalty) => Ok(ExtraInfo::StartingLoyalty(loyalty)),
                None          => Ok(ExtraInfo::None)
            }
        }
    }
}
fn read_mana(s: &String, card_name: &String) -> Result<Vec<Mana>, BuilderError> {
    let mut mana = Vec::new();
    let mut current = None;
    let mut is_half = false;
    let mut is_split = false;

    for c in s.chars() {
        match c.to_uppercase() {
            '{' if !is_half && !is_split => match current {
                Some(_) => return Err(BuilderError::InvalidCardField(card_name.clone(), "manaCost")),
                None    => {
                    current = Some(None)
                }
            },
            '}' if !is_half && !is_split => match current {
                Some(Some(m)) => {
                    current = None;
                    mana.push(m);
                },
                _ => return Err(BuilderError::InvalidCardField(card_name.clone(), "manaCost")),
            },
            'H' if !is_half && !is_split => match current {
                Some(None) => {
                    is_half = true;
                },
                _ => return Err(BuilderError::InvalidCardField(card_name.clone(), "manaCost"))
            },
            '/' if !is_half && !is_split => match current {
                Some(Some(Mana::Colored(_))) | Some(Some(Mana::Colorless(_))) => {
                    is_split = true;
                },
                _ => return Err(BuilderError::InvalidCardField(card_name.clone(), "manaCost"))
            },
            'W' | 'U' | 'B' | 'R' | 'G' => {
                let col = try!(read_color(c, card_name));

                match current {
                    Some(Some(Mana::Colored(oth))) if is_split => {
                        is_split = false;
                        current = Some(Some(Mana::Hybrid(oth, col)));
                    },
                    Some(Some(Mana::Colorless(val))) if is_split => {
                        is_split = false;
                        current = Some(Some(Mana::ColorlessHybrid(val, col)));
                    }
                    Some(None) if is_half => {
                        is_half = false;
                        current = Some(Some(Mana::Half(col)));
                    },
                    Some(None) if !is_half => {
                        current = Some(Some(Mana::Colored(col)));
                    }
                    _ => return Err(BuilderError::InvalidCardField(card_name.clone(), "manaCost"))
                }
            },
            num if num.is_digit(10) => {
                let num = num.to_digit(10).unwrap() as u32;

                match current {
                    Some(Some(Mana::Colorless(val))) => {
                        current = Some(Some(Mana::Colorless(val * 10 + num)))
                    },
                    Some(None) => {
                        current = Some(Some(Mana::Colorless(num)))
                    }
                    Some(Some(_)) | None => return Err(BuilderError::InvalidCardField(card_name.clone(), "manaCost"))
                }
            },
            _ => return Err(BuilderError::InvalidCardField(card_name.clone(), "manaCost"))
        }
    }

    Ok(mana)
}

fn read_card(js: &json::Json, card_name: &String) -> Result<CardInfo, BuilderError> {
    let card_obj = match js.as_object() {
        Some(card_obj) => card_obj,
        None           => return Err(BuilderError::InvalidCardObject(card_name.clone()))
    };

    Ok(CardInfo {
        name: try!(read_string(card_obj, "name", card_name)),
        mana: match try!(read_optional!(read_string, card_obj, "manaCost", card_name)) {
            Some(m) => Some(try!(read_mana(&m, card_name))),
            None    => None
        },
        color: try!(read_optional!(read_string_array, card_obj, "colors", card_name)),

        supertypes: try!(read_optional!(read_string_array, card_obj, "supertypes", card_name)),
        types: try!(read_optional!(read_string_array, card_obj, "types", card_name)),
        subtypes: try!(read_optional!(read_string_array, card_obj, "subtypes", card_name)),

        image_name: try!(read_string(card_obj, "imageName", card_name)),
        text: try!(read_optional!(read_string, card_obj, "text", card_name)),
        flavor_text: try!(read_optional!(read_string, card_obj, "flavorText", card_name)),

        extra: try!(read_extra(card_obj, card_name))
    })
}

pub fn from_json(js: &json::Json) -> Result<collections::HashMap<String, Card>, BuilderError> {
    let name_to_json = match js.as_object() {
        Some(name_to_json) => name_to_json,
        None               => return Err(BuilderError::NoTopLevelObject)
    };

    let mut name_to_cardinfo = collections::HashMap::new();

    for (k, v) in name_to_json.iter() {
        name_to_cardinfo.insert(k.clone(), try!(read_card(v, k)));
    }

    let name_to_cardinfo = name_to_cardinfo;
    let mut name_to_card = collections::HashMap::new();

    for (k, v) in name_to_cardinfo.iter() {
        let layout = try!(read_string(name_to_json.get(k).unwrap().as_object().unwrap(), "layout", k));

        match layout.as_slice() {
            "normal" => {
                name_to_card.insert(k.clone(), Card::Normal(rc::Rc::new(box v.clone())));
            },
            "split" | "flip" | "double-faced" => {
                if !name_to_card.contains_key(k) {
                    let names = try!(read_string_array(name_to_json.get(k).unwrap().as_object().unwrap(), "names", k));

                    if names.len() != 2 {
                        return Err(BuilderError::InvalidCardField(k.clone(), "names"));
                    }

                    let card_a = rc::Rc::new(box name_to_cardinfo.get(&names[0]).unwrap().clone());
                    let card_b = rc::Rc::new(box name_to_cardinfo.get(&names[1]).unwrap().clone());

                    let (card_a, card_b) = match layout.as_slice() {
                        "split"        => (Card::Split(card_a.clone(), card_b.clone()),
                                           Card::Split(card_b, card_a)),
                        "flip"         => (Card::Flip(card_a.clone(), card_b.clone()),
                                           Card::Flip(card_b, card_a)),
                        "double-faced" => (Card::DoubleFaced(card_a.clone(), card_b.clone()),
                                           Card::DoubleFaced(card_b, card_a)),
                        _ => panic!("This should never happen")
                    };

                    name_to_card.insert(names[0].clone(), card_a);
                    name_to_card.insert(names[1].clone(), card_b);
                }
            },
            _ => return Err(BuilderError::InvalidCardField(k.clone(), "layout"))
        }
    }

    Ok(name_to_card)
}
pub fn from_reader(rdr: &mut io::Reader) -> Result<collections::HashMap<String, Card>, BuilderError> {
    from_json(&(match json::from_reader(rdr) {
        Ok(js) => js,
        Err(e) => return Err(BuilderError::Json(e))
    }))
}
pub fn from_str(s: &str) -> Result<collections::HashMap<String, Card>, BuilderError> {
    from_json(&(match json::from_str(s) {
        Ok(js) => js,
        Err(e) => return Err(BuilderError::Json(e))
    }))
}

#[test]
fn load_test() {
    let json_str = r#"{
        "Air Elemental": {
            "layout": "normal",
            "name": "Air Elemental",
            "manaCost": "{3}{U}{U}",
            "cmc": 5,
            "colors": ["Blue"],
            "type": "Creature — Elemental",
            "types": ["Creature"],
            "subtypes": ["Elemental"],
            "text": "Flying",
            "power": "4",
            "toughness": "4",
            "imageName": "air elemental"
        },
        "Ashiok, Nightmare Weaver": {
            "layout": "normal",
            "name": "Ashiok, Nightmare Weaver",
            "manaCost": "{1}{U}{B}",
            "cmc": 3,
            "colors": ["Blue", "Black"],
            "type": "Planeswalker — Ashiok",
            "types": ["Planeswalker"],
            "subtypes": ["Ashiok"],
            "text": "+2: Exile the top three cards of target opponent's library.\n−X: Put a creature card with converted mana cost X exiled with Ashiok, Nightmare Weaver onto the battlefield under your control. That creature is a Nightmare in addition to its other types.\n−10: Exile all cards from all opponents' hands and graveyards.",
            "loyalty": 3,
            "imageName": "ashiok, nightmare weaver"
        },
        "Budoka Pupil": {
            "layout": "flip",
            "name": "Budoka Pupil",
            "names": ["Budoka Pupil", "Ichiga, Who Topples Oaks"],
            "manaCost": "{1}{G}{G}",
            "cmc": 3,
            "colors": ["Green"],
            "type": "Creature — Human Monk",
            "types": ["Creature"],
            "subtypes": ["Human", "Monk"],
            "text": "Whenever you cast a Spirit or Arcane spell, you may put a ki counter on Budoka Pupil.\nAt the beginning of the end step, if there are two or more ki counters on Budoka Pupil, you may flip it.",
            "power": "2",
            "toughness": "2",
            "imageName": "budoka pupil"
        },
        "Forest": {
            "layout": "normal",
            "name": "Forest",
            "type": "Basic Land — Forest",
            "supertypes": ["Basic"],
            "types": ["Land"],
            "subtypes": ["Forest"],
            "imageName": "forest"
        },
        "Ichiga, Who Topples Oaks": {
            "layout": "flip",
            "name": "Ichiga, Who Topples Oaks",
            "names": ["Budoka Pupil", "Ichiga, Who Topples Oaks"],
            "manaCost": "{1}{G}{G}",
            "cmc": 3,
            "colors": ["Green"],
            "type": "Legendary Creature — Spirit",
            "supertypes": ["Legendary"],
            "types": ["Creature"],
            "subtypes": ["Spirit"],
            "text": "Trample\nRemove a ki counter from Ichiga, Who Topples Oaks: Target creature gets +2/+2 until end of turn.",
            "power": "4",
            "toughness": "3",
            "imageName": "ichiga, who topples oaks"
        }
    }"#;
    let db = from_str(json_str);
    assert!(db.is_ok());
    let db = db.unwrap();
    let air_elemental = db.get(&"Air Elemental".to_string());
    let ashiok = db.get(&"Ashiok, Nightmare Weaver".to_string());
    let budoka = db.get(&"Budoka Pupil".to_string());
    let forest = db.get(&"Forest".to_string());
    assert!(air_elemental.is_some());
    assert!(budoka.is_some());
    assert!(ashiok.is_some());
    assert!(forest.is_some());
    let air_elemental = air_elemental.unwrap().clone();
    let budoka = budoka.unwrap().clone();
    let ashiok = ashiok.unwrap().clone();
    let forest = forest.unwrap().clone();
    assert!(match air_elemental {
        Card::Normal(air_elemental) => {
            assert_eq!(air_elemental.name.as_slice(), "Air Elemental");
            assert_eq!(air_elemental.mana.clone().unwrap(), vec![Mana::Colorless(3), Mana::Colored(Color::Blue), Mana::Colored(Color::Blue)]);
            assert!(match air_elemental.extra {
                ExtraInfo::PowerToughness(ref p, ref t) => {
                    assert!(p.as_slice() == "4");
                    assert!(t.as_slice() == "4");
                    true
                }
                _ => false
            })
                true
        },
        _ => false
    });
    assert!(match budoka {
        Card::Flip(budoka, ichiga) => {
            assert_eq!(budoka.name.as_slice(), "Budoka Pupil");
            assert_eq!(ichiga.name.as_slice(), "Ichiga, Who Topples Oaks");
            true
        },
        _ => false
    });
}
