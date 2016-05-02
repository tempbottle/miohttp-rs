use std;
use std::collections::HashMap;
use httparse;
use std::io::{Error, ErrorKind};


/*
http://hyper.rs/hyper/hyper/header/struct.Headers.html
                ta biblioteka wykorzystuje nagłówki dostarczane przez hyper-a
https://github.com/tailhook/rotor-http/blob/master/src/http1.rs
*/


pub struct Request {
    method      : String,
    path        : String,
    version     : u8,
    headers     : HashMap<Box<String>, String>,
}


impl Request {    

    pub fn new(req: httparse::Request) -> Result<Request, Error> {

        match (req.method, req.path, req.version) {

            (Some(method), Some(path), Some(version)) => {

                let mut headers = HashMap::new();

                for header in req.headers {

                    let key   = header.name.to_owned();

                    let value = match std::str::from_utf8(header.value) {
                        Ok(value) => value.to_owned(),
                        Err(err) => {
                            return Err(Error::new(ErrorKind::InvalidInput, format!("header {}, error utf8 sequence: {}", key, err)))
                        }
                    };

                    match headers.insert(Box::new(key.clone()), value) {
                        None => {}      //insert ok
                        Some(_) => {
                            return Err(Error::new(ErrorKind::InvalidInput, format!("double header: {}", &key)));
                        }
                    };
                }

                Ok(Request{
                    method      : method.to_owned(),
                    path        : path.to_owned(),
                    version     : version,
                    headers     : headers,
                })
            }
            _ => {

                                        //TODO - komunikat ma bardziej szczegółowo wskazywać gdzie wystąpił błąd
                Err(Error::new(ErrorKind::InvalidInput, "Błąd tworzenia odpowiedzi"))
            }
        }
    }
    
    pub fn is_header_set(&self, name: &str, value: &str) -> bool {
        
        match self.headers.get(&Box::new(name.to_owned())) {
            
            Some(get_value) => {
                get_value == value.trim()
            }
            
            None => false
        }
    }

    pub fn path(&self) -> &String {
        &(self.path)
    }
    
    pub fn is_post(&self) -> bool {
        self.method == "POST".to_owned()
    }
    
    pub fn method(&self) -> &String {
        &(self.method)
    }
    
    pub fn version(&self) -> u8 {
        self.version.clone()
    }
    
    pub fn get_content_length(&self) -> Option<usize> {
        
        match self.headers.get(&Box::new("Content-Length".to_owned())) {
            
            Some(value) => {
                
                match value.parse() {
                    Ok(value_parsed) => Some(value_parsed),
                    Err(_) => None,
                }
            },
            
            None => None
        }
        
    }
}


