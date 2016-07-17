extern crate hyper;
extern crate rustc_serialize;
extern crate xml;

use self::rustc_serialize::Encodable;
use std;
use std::io::Read;
use super::serde;

pub struct Client {
    http_client: hyper::Client,
    server_uri: String,
}

impl Client {
    pub fn new(server_uri: &str) -> Client {
        Client {
            http_client: hyper::Client::new(),
            server_uri: server_uri.to_owned(),
        }
    }

    pub fn request(&self, function_name: &str, parameters: &[&str]) -> ClientResult {
        let mut body = Vec::<u8>::new();
        {
            let mut encoder = serde::Encoder::new(&mut body);
            try!(encoder.start_request(function_name));
            for param in parameters {
                try!(param.encode(&mut encoder));
            }
            try!(encoder.end_request());
        }

        let body = try!(String::from_utf8(body));
        let res = try!(self.http_client
            .post(&self.server_uri)
            .body(&body)
            .send());

        let xml_tree = try!(read_xml_tree(&mut xml::EventReader::new(res)).ok_or(Error::Parse));
        parse_xml_tree(&xml_tree).ok_or(Error::Parse)
    }
}

fn read_xml_tree<T: Read>(parser: &mut xml::EventReader<T>) -> Option<XmlTreeNode> {
    match parser.next() {
        Ok(xml::reader::XmlEvent::StartElement { name, .. }) => {
            let mut children = Vec::<XmlTreeNode>::new();
            while let Some(tree) = read_xml_tree(parser) {
                children.push(tree);
            }
            Some(XmlTreeNode::Node(name.local_name, children))
        }
        Ok(xml::reader::XmlEvent::Characters(value)) => Some(XmlTreeNode::Leaf(value)),
        Ok(xml::reader::XmlEvent::EndElement { .. }) => None,
        Err(..) => None,
        _ => read_xml_tree(parser),
    }
}

fn parse_xml_tree(tree: &XmlTreeNode) -> Option<Member> {
    if let Some(tree) = peel_xml_layer(tree, "methodResponse") {
        if let Some(tree) = peel_xml_layer(tree, "params") {
            if let Some(tree) = peel_xml_layer(tree, "param") {
                return parse_xml_tree_helper(tree);
            }
        }
    }
    None
}

fn parse_xml_tree_helper(tree: &XmlTreeNode) -> Option<Member> {
    if let Some(tree) = peel_xml_layer(tree, "value") {
        if let XmlTreeNode::Node(ref name, ref children) = *tree {
            if children.len() == 1 {
                let child = &children[0];
                return match name.as_str() {
                    "array" => parse_xml_array(child),
                    "string" => parse_xml_string(child),
                    "int" | "i4" => parse_xml_int(child),
                    _ => None,
                };
            }
        }
    }
    None
}

fn parse_xml_array(tree: &XmlTreeNode) -> Option<Member> {
    if let XmlTreeNode::Node(ref name, ref children) = *tree {
        if name.as_str() == "data" {
            return Some(Member::Array(children.iter().filter_map(parse_xml_tree_helper).collect()));
        }
    }
    None
}

fn parse_xml_int(tree: &XmlTreeNode) -> Option<Member> {
    if let Some(Member::String(text)) = parse_xml_string(tree) {
        if let Ok(value) = text.parse::<i32>() {
            return Some(Member::Int(value));
        }
    }
    None
}

fn parse_xml_string(tree: &XmlTreeNode) -> Option<Member> {
    if let XmlTreeNode::Leaf(ref value) = *tree {
        return Some(Member::String(value.clone()));
    }
    None
}

fn peel_xml_layer<'a>(tree: &'a XmlTreeNode, node_name: &str) -> Option<&'a XmlTreeNode> {
    if let XmlTreeNode::Node(ref name, ref children) = *tree {
        if name.as_str() == node_name && children.len() == 1 {
            return Some(&children[0]);
        }
    }
    None
}

enum XmlTreeNode {
    Leaf(String),
    Node(String, Vec<XmlTreeNode>),
}

pub enum Member {
    Array(Vec<Member>),
    String(String),
    Int(i32),
}

pub type ClientResult = Result<Member, Error>;

#[derive(Debug)]
pub enum Error {
    Http(hyper::error::Error),
    Utf8(std::string::FromUtf8Error),
    Serialization(serde::encoder::Error),
    XmlRead(xml::reader::Error),
    Parse,
}

impl From<hyper::error::Error> for Error {
    fn from(err: hyper::error::Error) -> Error {
        Error::Http(err)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(err: std::string::FromUtf8Error) -> Error {
        Error::Utf8(err)
    }
}

impl From<serde::encoder::Error> for Error {
    fn from(err: serde::encoder::Error) -> Error {
        Error::Serialization(err)
    }
}

impl From<xml::reader::Error> for Error {
    fn from(err: xml::reader::Error) -> Error {
        Error::XmlRead(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::Http(ref err) => write!(f, "HTTP error: {}", err),
            Error::Utf8(ref err) => write!(f, "UTF8 error: {}", err),
            Error::Serialization(ref err) => write!(f, "Serialization error: {}", err),
            Error::XmlRead(ref err) => write!(f, "XML reading error: {}", err),
            Error::Parse => write!(f, "XMLRPC response parsing error"),
        }
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Http(ref err) => err.description(),
            Error::Utf8(ref err) => err.description(),
            Error::Serialization(ref err) => err.description(),
            Error::XmlRead(ref err) => err.description(),
            Error::Parse => "Could not parse XMLRPC response",
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        match *self {
            Error::Http(ref err) => Some(err),
            Error::Utf8(ref err) => Some(err),
            Error::Serialization(ref err) => Some(err),
            Error::XmlRead(ref err) => Some(err),
            Error::Parse => None,
        }
    }
}
