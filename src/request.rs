use std;
use std::collections::HashMap;
use httparse;
use std::io::{Error, ErrorKind};
use mio::{Token, Sender};
use server::MioMessage;
use response::Response;

use std::boxed::FnBox;

/*
http://hyper.rs/hyper/hyper/header/struct.Headers.html
                ta biblioteka wykorzystuje nagłówki dostarczane przez hyper-a
https://github.com/tailhook/rotor-http/blob/master/src/http1.rs
*/

pub struct PreRequest {
    method  : String,
    path    : String,
    version : u8,
    headers : HashMap<Box<String>, String>,
}

impl PreRequest { 

    pub fn new(req: httparse::Request) -> Result<PreRequest, Error> {
        
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

                Ok(PreRequest{
                    method  : method.to_owned(),
                    path    : path.to_owned(),
                    version : version,
                    headers : headers,
                })
            }
            _ => {

                                        //TODO - komunikat ma bardziej szczegółowo wskazywać gdzie wystąpił błąd
                Err(Error::new(ErrorKind::InvalidInput, "Błąd tworzenia odpowiedzi"))
            }
        }
    }
    
    pub fn bind(self, token: Token, sender :  Sender<MioMessage>) -> Request {
        
        Request {
            is_send     : false,
            pre_request : self,
            token       : token,
            sender      : sender,
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
    
    pub fn get_header(&self, header: String) -> Option<usize> {
        
        match self.headers.get(&Box::new(header)) {
            
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


pub struct Request {
    is_send     : bool,
    pre_request : PreRequest,
    token       : Token,
    sender      : Sender<MioMessage>,
}


impl Request {    

    pub fn get_post(self, callback: Box<FnBox(Request, Option<Vec<u8>>) + Send + Sync + 'static>) {
        
        let token  = self.token.clone();
        let sender = self.sender.clone();
        
        sender.send(MioMessage::GetPost(token, self, callback)).unwrap();
    }
    
    pub fn path(&self) -> &String {
        self.pre_request.path()
    }
    
    pub fn version(&self) -> u8 {
        self.pre_request.version()
    }
    
    pub fn method(&self) -> &String {
        self.pre_request.method()
    }
    
    pub fn is_post(&self) -> bool {
        self.pre_request.is_post()
    }
    
    pub fn send(mut self, response: Response) {
        
        self.is_send = true;
        
        (self.sender).send(MioMessage::Response(self.token, response)).unwrap();
    }
}


impl Drop for Request {
    
    fn drop(&mut self) {
        
        if self.is_send == false {
            
            let resp500 = Response::create_500();
            (self.sender).send(MioMessage::Response(self.token, resp500)).unwrap();
        }
    }
}


                                                                   //task gwarantuje drop-a
            /*
            worker::render_request(api_file, request, Task::new(Box::new(move|result : Option<(Response)>|{

                match result {

                    Some(resp) => {

                        respchan.send(resp);
                    },

                    None => {
                                                                //coś poszło nie tak z obsługą tego requestu
                        respchan.send(Response::create_500());
                    }
                };
            })));
            */
