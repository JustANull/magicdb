#![crate_name = "magicdb"]
#![crate_type = "lib"]

#![feature(macro_rules)]

extern crate serialize;

use serialize::json;
use std::ascii::AsciiExt;
use std::collections;
use std::io;

#[deriving(Clone, PartialEq, Eq, Show)]
pub struct Card {
    name:  String,
    mana:  Option<Vec<Mana>>,
    color: Option<Vec<Color>>,

    layout:     CardLayout,
    other_side: Option<String>,

    supertypes: Option<Vec<String>>,
    types:      Option<Vec<String>>,
    subtypes:   Option<Vec<String>>,

    image_name:  String,
    text:        Option<String>,
    flavor_text: Option<String>,

    extra: ExtraInfo
}
#[deriving(Clone, PartialEq, Eq, Show)]
pub enum CardLayout {
    Normal, Split, Flip, DoubleFaced
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
pub enum CardError {
    NoCardField(&'static str),
    InvalidCardField(&'static str)
}
#[deriving(Clone, PartialEq, Show)]
pub enum BuilderError {
    NoTopLevelObject,
    InvalidCardObject(String),
    Named(String, CardError),
    Json(json::BuilderError)
}

fn read_integer(js: &json::JsonObject, field: &'static str) -> Result<i64, CardError> {
    match js.get(&field.to_string()) {
        Some(obj) => match obj.as_i64() {
            Some(i) => Ok(i),
            None    => Err(CardError::InvalidCardField(field))
        },
        None => Err(CardError::NoCardField(field))
    }
}
fn read_string(js: &json::JsonObject, field: &'static str) -> Result<String, CardError> {
    match js.get(&field.to_string()) {
        Some(obj) => match obj.as_string() {
            Some(s) => Ok(s.to_string()),
            None    => Err(CardError::InvalidCardField(field))
        },
        None => Err(CardError::NoCardField(field))
    }
}
fn read_string_array(js: &json::JsonObject, field: &'static str) -> Result<Vec<String>, CardError> {
    match js.get(&field.to_string()) {
        Some(obj) => match obj.as_array() {
            Some(arr) => {
                let mut s_vec = Vec::new();

                for arr_obj in arr.iter() {
                    match arr_obj.as_string() {
                        Some(s) => {
                            s_vec.push(s.to_string())
                        },
                        None => return Err(CardError::InvalidCardField(field))
                    }
                }

                Ok(s_vec)
            },
            None => Err(CardError::InvalidCardField(field))
        },
        None => Err(CardError::NoCardField(field))
    }
}

fn read_color_ch(c: char) -> Option<Color> {
    match c.to_uppercase() {
        'W' => Some(Color::White),
        'U' => Some(Color::Blue),
        'B' => Some(Color::Black),
        'R' => Some(Color::Red),
        'G' => Some(Color::Green),
        _   => None,
    }
}
fn read_color_st(s: &String) -> Option<Color> {
    if s.eq_ignore_ascii_case("White") {
        Some(Color::White)
    } else if s.eq_ignore_ascii_case("Blue") {
        Some(Color::Blue)
    } else if s.eq_ignore_ascii_case("Black") {
        Some(Color::Black)
    } else if s.eq_ignore_ascii_case("Red") {
        Some(Color::Red)
    } else if s.eq_ignore_ascii_case("Green") {
        Some(Color::Green)
    } else {
        None
    }
}
fn read_mana_st(s: &str) -> Result<Vec<Mana>, CardError> {
    let mut mana = Vec::new();
    let mut current = None;
    let mut is_half = false;
    let mut is_split = false;

    for c in s.chars() {
        match c.to_uppercase() {
            '{' if !is_half && !is_split => match current {
                Some(_) => return Err(CardError::InvalidCardField("manaCost")),
                None    => {
                    current = Some(None)
                }
            },
            '}' if !is_half && !is_split => match current {
                Some(Some(m)) => {
                    current = None;
                    mana.push(m);
                },
                _ => return Err(CardError::InvalidCardField("manaCost")),
            },
            'H' if !is_half && !is_split => match current {
                Some(None) => {
                    is_half = true;
                },
                _ => return Err(CardError::InvalidCardField("manaCost"))
            },
            '/' if !is_half && !is_split => match current {
                Some(Some(Mana::Colored(_))) | Some(Some(Mana::Colorless(_))) => {
                    is_split = true;
                },
                _ => return Err(CardError::InvalidCardField("manaCost"))
            },
            'W' | 'U' | 'B' | 'R' | 'G' => {
                let col = match read_color_ch(c) {
                    Some(col) => col,
                    None      => return Err(CardError::InvalidCardField("manaCost"))
                };

                match current {
                    Some(Some(Mana::Colored(oth))) if is_split => {
                        is_split = false;
                        current = Some(Some(Mana::Hybrid(oth, col)));
                    },
                    Some(Some(Mana::Colorless(val))) if is_split => {
                        is_split = false;
                        current = Some(Some(Mana::ColorlessHybrid(val, col)));
                    },
                    Some(None) if is_half => {
                        is_half = false;
                        current = Some(Some(Mana::Half(col)));
                    },
                    Some(None) if !is_half => {
                        current = Some(Some(Mana::Colored(col)));
                    }
                    _ => return Err(CardError::InvalidCardField("manaCost"))
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
                    Some(Some(_)) | None => return Err(CardError::InvalidCardField("manaCost"))
                }
            },
            _ => return Err(CardError::InvalidCardField("manaCost"))
        }
    }

    Ok(mana)
}

macro_rules! read_optional(
    ($f:expr, $js:expr, $field:expr) => (
        match $f($js, $field) {
            Ok(f)  => Ok(Some(f)),
            Err(f) => match f {
                CardError::NoCardField(_) => Ok(None),
                _                            => Err(f)
            }
        }
    );
)

fn read_color(js: &json::JsonObject) -> Result<Option<Vec<Color>>, CardError> {
    match try!(read_optional!(read_string_array, js, "colors")) {
        Some(a) => {
            let mut arr = Vec::new();

            for s in a.iter() {
                arr.push(match read_color_st(s) {
                    Some(col) => col,
                    None      => return Err(CardError::InvalidCardField("colors"))
                });
            }

            Ok(Some(arr))
        },
        None => Ok(None)
    }
}
fn read_extra(js: &json::JsonObject) -> Result<ExtraInfo, CardError> {
    match try!(read_optional!(read_string, js, "power")) {
        Some(power) => Ok(ExtraInfo::PowerToughness(power, try!(read_string(js, "toughness")))),
        None        => {
            match try!(read_optional!(read_integer, js, "loyalty")) {
                Some(loyalty) => Ok(ExtraInfo::StartingLoyalty(loyalty)),
                None          => Ok(ExtraInfo::None)
            }
        }
    }
}
fn read_layout(js: &json::JsonObject) -> Result<CardLayout, CardError> {
    match try!(read_string(js, "layout")).as_slice() {
        "normal"       => Ok(CardLayout::Normal),
        "split"        => Ok(CardLayout::Split),
        "flip"         => Ok(CardLayout::Flip),
        "double-faced" => Ok(CardLayout::DoubleFaced),
        _              => Err(CardError::InvalidCardField("layout"))
    }
}
fn read_mana(js: &json::JsonObject) -> Result<Option<Vec<Mana>>, CardError> {
    match try!(read_optional!(read_string, js, "manaCost")) {
        Some(s) => Ok(Some(try!(read_mana_st(s.as_slice())))),
        None    => Ok(None)
    }
}
fn read_other_side(js: &json::JsonObject, layout: CardLayout, card_name: &str) -> Result<Option<String>, CardError> {
    match layout {
        CardLayout::Normal => Ok(None),
        CardLayout::Split | CardLayout::Flip | CardLayout::DoubleFaced => {
            let mut names = try!(read_string_array(js, "names"));

            if names.len() != 2 {
                return Err(CardError::InvalidCardField("names"));
            }

            let name_2 = names.pop().unwrap();
            let name_1 = names.pop().unwrap();

            if name_1.as_slice() == card_name {
                Ok(Some(name_2))
            } else if name_2.as_slice() == card_name {
                Ok(Some(name_1))
            } else {
                Err(CardError::InvalidCardField("names"))
            }
        }
    }
}

macro_rules! dec_try(
    ($name:expr, $e:expr) => (
        match $e {
            Ok(e)  => e,
            Err(e) => return Err(BuilderError::Named($name.to_string(), e))
        }
    );
)

fn read_card(card_obj: &json::JsonObject, card_name: &str) -> Result<Card, BuilderError> {
    let name  = dec_try!(card_name, read_string(card_obj, "name"));
    let mana  = dec_try!(card_name, read_mana(card_obj));
    let color = dec_try!(card_name, read_color(card_obj));

    let layout     = dec_try!(card_name, read_layout(card_obj));
    let other_side = dec_try!(card_name, read_other_side(card_obj, layout, name.as_slice()));

    let supertypes = dec_try!(card_name, read_optional!(read_string_array, card_obj, "supertypes"));
    let types      = dec_try!(card_name, read_optional!(read_string_array, card_obj, "types"));
    let subtypes   = dec_try!(card_name, read_optional!(read_string_array, card_obj, "subtypes"));

    let image_name  = dec_try!(card_name, read_string(card_obj, "imageName"));
    let text        = dec_try!(card_name, read_optional!(read_string, card_obj, "text"));
    let flavor_text = dec_try!(card_name, read_optional!(read_string, card_obj, "flavorText"));

    let extra = dec_try!(card_name, read_extra(card_obj));

    Ok(Card {
        name:  name,
        mana:  mana,
        color: color,

        layout:     layout,
        other_side: other_side,

        supertypes: supertypes,
        types:      types,
        subtypes:   subtypes,

        image_name:  image_name,
        text:        text,
        flavor_text: flavor_text,

        extra: extra
    })
}

pub fn from_json(js: &json::Json) -> Result<collections::HashMap<String, Card>, BuilderError> {
    let name_to_json = match js.as_object() {
        Some(name_to_json) => name_to_json,
        None               => return Err(BuilderError::NoTopLevelObject)
    };

    let mut name_to_card = collections::HashMap::new();

    for (k, v) in name_to_json.iter() {
        name_to_card.insert(k.clone(), try!(read_card(match v.as_object() {
            Some(card_obj) => card_obj,
            None           => return Err(BuilderError::InvalidCardObject(k.clone()))
	    }, k.as_slice())));
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
    assert_eq!(air_elemental.name.as_slice(), "Air Elemental");
    assert_eq!(air_elemental.mana.clone().unwrap(), vec![Mana::Colorless(3), Mana::Colored(Color::Blue), Mana::Colored(Color::Blue)]);
    assert!(match air_elemental.extra {
        ExtraInfo::PowerToughness(ref p, ref t) => {
            assert!(p.as_slice() == "4");
            assert!(t.as_slice() == "4");
            true
        }
        _ => false
    });
    assert_eq!(budoka.name.as_slice(), "Budoka Pupil");
    assert_eq!(budoka.other_side.unwrap().as_slice(), "Ichiga, Who Topples Oaks");
}
