#[derive(Debug,RustcDecodable,RustcEncodable)]
pub struct Topic {
    pub name: String,
    pub datatype: String,
}
