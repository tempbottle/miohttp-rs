use typemod::Type;
use code::Code;


#[derive(Debug)]
pub struct Response {
    close_connection : bool,
    message          : Vec<u8>,
}


impl Response {
    
    pub fn as_bytes(self) -> Vec<u8> {
        self.message
    }
    
    fn append_string(&mut self, line: String) {
        self.message.append(&mut (line + "\r\n").into_bytes());
    }
    
    pub fn close_connection(&self) -> bool {
        self.close_connection
    }
    
    fn create_headers(code: Code, typ: Type, length: usize) -> Response {
        
        let close_connection = code == Code::Code500 || code == Code::Code400;
        
        let mut response = Response {
            close_connection : close_connection,
            message          : Vec::new(),
        };
        
        response.append_string("HTTP/1.1 ".to_owned() + code.to_str());
        response.append_string("Date: Thu, 20 Dec 2001 12:04:30 GMT".to_owned());
        response.append_string("Content-Type: ".to_owned() + typ.to_str());
        response.append_string("Connection: keep-alive".to_owned());
        response.append_string(format!("Content-length: {}", length).to_owned());
        response.append_string("".to_owned());
        
        response
    }
    
    pub fn create(code: Code, typ: Type, body: String) -> Response {
        
        let mut response = Response::create_headers(code, typ, body.len());
        
        response.append_string(body);
        
        response
    }
    
    pub fn create_from_buf(code: Code, typ: Type, mut body: Vec<u8>) -> Response {
        
        let mut response = Response::create_headers(code, typ, body.len());
        
        response.message.append(&mut body);
        
        response
    }
    
    pub fn create_500() -> Response {
        Response::create(Code::Code500, Type::TextHtml, "500 Internal Server Error".to_owned())
    }
    
    pub fn create_400() -> Response {
        Response::create(Code::Code400, Type::TextHtml, "400 Bad Request".to_owned())
    }
    
    /*
    let mut out: Vec<u8> = Vec::new();
    out.append(&mut ("HTTP/1.1 ".to_owned() + code.to_str() + "\r\n").into_bytes());
    out.append(&mut ("Date: Thu, 20 Dec 2001 12:04:30 GMT".to_owned() + "\r\n").into_bytes());

    out.append(&mut ("Content-Type: ".to_owned() + typ.to_str() + "\r\n").into_bytes());
    out.append(&mut ("Connection: keep-alive".to_owned() + "\r\n").into_bytes());
    out.append(&mut (format!("Content-length: {}", body.len()).to_owned() + "\r\n").into_bytes());
    out.append(&mut ("\r\n".to_owned().into_bytes()));
    out.append(&mut (body.into_bytes()));
    */
    /*
    Response {
        message : out
    }*/

    /*
    let mut out: Vec<String> = Vec::new();
    out.push("HTTP/1.1 ".to_owned() + code.to_str());
    out.push("Date: Thu, 20 Dec 2001 12:04:30 GMT".to_owned());      //TODO - trzeba wyznaczać aktualną wartość daty
    out.push("Content-Type: ".to_owned() + typ.to_str());
    out.push("Connection: keep-alive".to_owned());
    out.push(format!("Content-length: {}", body.len()).to_owned());
    out.push("".to_owned());
    out.push(body);

    let message = out.join("\r\n");

    let mut resp_vec: Vec<u8> = Vec::new();

    for byte in message.as_bytes() {
        resp_vec.push(byte.clone());
    }
    */

    /*
    let mut vec = vec![1, 2, 3];
    let mut vec2 = vec![4, 5, 6];
    vec.append(&mut vec2);
    assert_eq!(vec, [1, 2, 3, 4, 5, 6]);
    assert_eq!(vec2, []);
    */

    /*
        konwertuje string na tablicę bajtów

    let s = String::from("hello");
    let bytes = s.into_bytes();
    assert_eq!(bytes, [104, 101, 108, 108, 111]);
    */ 
    
    /*
    println!("dd {}", ["hello", "world"].join(" "));
    println!("tt {}", ["asda 11".to_owned(), "asda 22".to_owned()].join(" "));
    
    let hello = "Hello ".to_owned();
    let world = "world!";
    let hello_world = hello + world;

    let hello = "Hello ".to_owned();
    let world = "world!".to_owned();
    let hello_world = hello + &world;
    
    "sfsfsd".to_owned() == "das"
    */
}

