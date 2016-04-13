pub enum Code {
    Code200,
    Code400,
    Code404,
    Code500,
}

//https://en.wikipedia.org/wiki/List_of_HTTP_status_codes
//https://doc.rust-lang.org/std/path/struct.Path.html#method.extension

impl Code {
    pub fn to_str(&self) -> &str {
        match *self {
            Code::Code200 => "200 OK",
            Code::Code400 => "400 Bad Request",
            Code::Code404 => "404 Not Found",
            Code::Code500 => "500 Internal Server Error",
        }
    }
}
