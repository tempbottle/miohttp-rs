use std;
use std::collections::HashMap;
use httparse;
use std::io::{Error, ErrorKind};
use std::sync::Arc;


pub struct Request {
    inner : Arc<RequestInner>,
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
                    inner : Arc::new(RequestInner{
                        method      : method.to_owned(),
                        path        : path.to_owned(),
                        version     : version,
                        headers     : headers,
                    })
                })
            }
            _ => {

                                        //TODO - komunikat ma bardziej szczegółowo wskazywać gdzie wystąpił błąd
                Err(Error::new(ErrorKind::InvalidInput, "Błąd tworzenia odpowiedzi"))
            }
        }
    }
    
    
    pub fn is_header_set(&self, name: &str, value: &str) -> bool {
        self.inner.is_header_set(name, value)
    }
    
    pub fn path(&self) -> &String {
        self.inner.path()
    }
    
    pub fn method(&self) -> &String {
        self.inner.method()
    }
    
    pub fn version(&self) -> u8 {
        self.inner.version()
    }
}


impl Clone for Request {
    
    fn clone(&self) -> Request {
        
        Request {
            inner : self.inner.clone(),
        }
    }

    fn clone_from(&mut self, request: &Request) {
        self.inner = request.inner.clone();
    }
}


struct RequestInner {
    method      : String,
    path        : String,
    version     : u8,
    headers     : HashMap<Box<String>, String>,
}

/*
http://hyper.rs/hyper/hyper/header/struct.Headers.html
                ta biblioteka wykorzystuje nagłówki dostarczane przez hyper-a
https://github.com/tailhook/rotor-http/blob/master/src/http1.rs
*/


impl RequestInner {

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
    
    pub fn method(&self) -> &String {
        &(self.method)
    }
    
    pub fn version(&self) -> u8 {
        self.version.clone()
    }
}


