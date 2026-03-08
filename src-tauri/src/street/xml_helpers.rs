use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

/// A parsed node from the Glitch typed-value XML system.
/// The XML uses elements like <int id="key">value</int>, <str id="key">value</str>,
/// <object id="key">children</object>, etc.
#[derive(Debug, Clone)]
pub enum XmlValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Object(HashMap<String, XmlValue>),
    ObjRef { tsid: String, label: String },
}

impl XmlValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            XmlValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            XmlValue::Int(v) => Some(*v as f64),
            XmlValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            XmlValue::Str(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            XmlValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, XmlValue>> {
        match self {
            XmlValue::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&XmlValue> {
        self.as_object()?.get(key)
    }
}

/// Parse a Glitch location XML string into an XmlValue tree.
pub fn parse_glitch_xml(xml: &str) -> Result<XmlValue, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    // Skip to game_object root, then parse its children
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "game_object" || tag == "object" {
                    return parse_object_children(&mut reader, &tag);
                }
            }
            Ok(Event::Eof) => return Err("Unexpected end of XML".into()),
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
    }
}

fn parse_object_children(reader: &mut Reader<&[u8]>, parent_tag: &str) -> Result<XmlValue, String> {
    let mut map = HashMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let id = get_attr(e, "id").unwrap_or_default();

                match tag.as_str() {
                    "object" => {
                        let value = parse_object_children(reader, "object")?;
                        if !id.is_empty() {
                            map.insert(id, value);
                        }
                    }
                    "int" => {
                        let text = read_text(reader, "int")?;
                        if let Ok(v) = text.parse::<i64>() {
                            map.insert(id, XmlValue::Int(v));
                        }
                    }
                    "str" => {
                        let text = read_text(reader, "str")?;
                        map.insert(id, XmlValue::Str(text));
                    }
                    "bool" => {
                        let text = read_text(reader, "bool")?;
                        map.insert(id, XmlValue::Bool(text == "true"));
                    }
                    "objref" => {
                        let tsid = get_attr(e, "tsid").unwrap_or_default();
                        let label = get_attr(e, "label").unwrap_or_default();
                        map.insert(id, XmlValue::ObjRef { tsid, label });
                        // objref may be self-closing or have an end tag
                        skip_to_end(reader, "objref");
                    }
                    "objrefs" => {
                        // Container of objref elements — parse as object
                        let value = parse_object_children(reader, "objrefs")?;
                        if !id.is_empty() {
                            map.insert(id, value);
                        }
                    }
                    _ => {
                        // Unknown element — use skip_to_end to safely consume
                        // any nested children, preventing stream corruption.
                        skip_to_end(reader, &tag);
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let id = get_attr(e, "id").unwrap_or_default();
                match tag.as_str() {
                    "null" => {
                        map.insert(id, XmlValue::Null);
                    }
                    "objref" => {
                        let tsid = get_attr(e, "tsid").unwrap_or_default();
                        let label = get_attr(e, "label").unwrap_or_default();
                        map.insert(id, XmlValue::ObjRef { tsid, label });
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == parent_tag {
                    return Ok(XmlValue::Object(map));
                }
            }
            Ok(Event::Eof) => return Err("Unexpected end of XML in object".into()),
            Err(e) => return Err(format!("XML error: {}", e)),
            _ => {}
        }
    }
}

fn get_attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name.as_bytes())
        .map(|a| String::from_utf8_lossy(&a.value).to_string())
}

fn read_text(reader: &mut Reader<&[u8]>, end_tag: &str) -> Result<String, String> {
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(e)) => {
                text = e.unescape().map_err(|e| e.to_string())?.to_string();
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == end_tag {
                    return Ok(text);
                }
            }
            Ok(Event::Eof) => return Err(format!("Unexpected EOF reading <{}>", end_tag)),
            Err(e) => return Err(format!("XML error in <{}>: {}", end_tag, e)),
            _ => {}
        }
    }
}

fn skip_to_end(reader: &mut Reader<&[u8]>, tag: &str) {
    let mut depth = 1;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(ref e)) => {
                let t = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if t == tag {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => return,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_typed_values() {
        let xml = r#"
        <object id="root">
            <int id="width">6000</int>
            <str id="name">Test Street</str>
            <bool id="active">true</bool>
            <null id="nothing"/>
        </object>"#;
        let val = parse_glitch_xml(xml).unwrap();
        assert_eq!(val.get("width").unwrap().as_int(), Some(6000));
        assert_eq!(val.get("name").unwrap().as_str(), Some("Test Street"));
        assert_eq!(val.get("active").unwrap().as_bool(), Some(true));
        assert!(matches!(val.get("nothing"), Some(XmlValue::Null)));
    }

    #[test]
    fn parse_nested_objects() {
        let xml = r#"
        <object id="root">
            <object id="gradient">
                <str id="top">87A8C9</str>
                <str id="bottom">FFC400</str>
            </object>
        </object>"#;
        let val = parse_glitch_xml(xml).unwrap();
        let grad = val.get("gradient").unwrap();
        assert_eq!(grad.get("top").unwrap().as_str(), Some("87A8C9"));
        assert_eq!(grad.get("bottom").unwrap().as_str(), Some("FFC400"));
    }

    #[test]
    fn parse_platform_line() {
        let xml = r#"
        <object id="root">
            <object id="platform_lines">
                <object id="plat_1">
                    <object id="start">
                        <int id="x">-228</int>
                        <int id="y">-58</int>
                    </object>
                    <object id="end">
                        <int id="x">-119</int>
                        <int id="y">-77</int>
                    </object>
                    <int id="platform_pc_perm">-1</int>
                </object>
            </object>
        </object>"#;
        let val = parse_glitch_xml(xml).unwrap();
        let plats = val.get("platform_lines").unwrap().as_object().unwrap();
        let p = plats.get("plat_1").unwrap();
        let start = p.get("start").unwrap();
        assert_eq!(start.get("x").unwrap().as_int(), Some(-228));
        assert_eq!(start.get("y").unwrap().as_int(), Some(-58));
    }
}
