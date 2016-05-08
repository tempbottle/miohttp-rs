use mio::{EventSet, TryRead, TryWrite};
use mio::tcp::{TcpStream};
use httparse;
use server::Event;
use request::PreRequest;
use response;
use std::cmp::min;
use request::Request;

use std::boxed::FnBox;

enum ConnectionMode {

                                                    //czytanie requestu
    ReadingRequest([u8; 2048], usize),
                                                    //oczekiwanie na wygenerowanie odpowiedzi serwera (bool to keep alive)
    WaitingForServerResponse(bool, ConnectionPost),
                                                    //wysyłanie odpowiedz (bool to keep alive)
    SendingResponse(bool, Vec<u8>, usize),
}

enum ConnectionPost {
    
    None,
    Data(Vec<u8>, usize),
    Reading(Vec<u8>, usize, Request, Box<FnBox(Request, Option<Vec<u8>>) + Send + Sync + 'static>),
    Complete,
}

pub enum TimerMode {
    In,                 //czytanie nagłówków requestu
    Out,                //wysyłanie danych do przeglądarki
    Post,               //czytanie parametrów post-a
    None,
}



pub enum LogMessage {
    Message(String),
    Error(String),
    None,
}


pub struct Connection {
    pub stream: TcpStream,
    mode: ConnectionMode,
}


impl Connection {


    pub fn new(stream: TcpStream) -> Connection {

        Connection {
            stream : stream,
            mode   : ConnectionMode::ReadingRequest([0u8; 2048], 0),
        }
    }

    fn make(stream: TcpStream, mode: ConnectionMode) -> Connection {

        Connection {
            stream : stream,
            mode   : mode,
        }
    }
    
    
    pub fn send_data_to_user(self, response: response::Response) -> (Connection, LogMessage) {
        
        match self.mode {

            ConnectionMode::WaitingForServerResponse(keep_alive, connection_post) => {
                
                let connection_post_close = match connection_post {
                    
                    ConnectionPost::None => false,
                    ConnectionPost::Data(_, _) => true,         //nieodebrane dane z posta, zamknij połączenie
                    ConnectionPost::Reading(_,_,_,_) => {
                        unreachable!();
                    },
                    ConnectionPost::Complete => false,
                };
                
                let new_keep_alive = if response.close_connection() || connection_post_close {
                    false
                } else {
                    keep_alive
                };
                
                let conn = Connection {
                    stream : self.stream,
                    mode   : ConnectionMode::SendingResponse(new_keep_alive, response.as_bytes(), 0),
                };
                
                (conn, LogMessage::None)
            }

            _ => {
                
                (self, LogMessage::Error("send_data_to_user: incorect state".to_owned()))
            }
        }

        
    }

    
    pub fn timeout_trigger(self) -> (Result<Connection, TcpStream>, LogMessage) {
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(_,_) => {
                
                (Err(self.stream), LogMessage::Message("timeout trigger - reading request".to_owned()))
            },
            
            ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Reading(_, _, request, callback)) => {
                
                (callback as Box<FnBox(Request, Option<Vec<u8>>)>)(request, None);
                
                let new_mode = ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Complete);

                (Ok(Connection::make(self.stream, new_mode)), LogMessage::Message("timeout trigger - reading post data".to_owned()))
            },
            
            ConnectionMode::WaitingForServerResponse(_,_) => {
                unreachable!();
            },
            
            ConnectionMode::SendingResponse(_,_,_) => {
                
                (Err(self.stream), LogMessage::Message("timeout trigger - sending request".to_owned()))
            }
        }
    }
    
    
    pub fn get_event(&self) -> Event {

        match self.mode {
            
            ConnectionMode::ReadingRequest(_, _) => Event::Read,
            
            ConnectionMode::WaitingForServerResponse(_, ref connection_post) => {
                
                match *connection_post {
                    ConnectionPost::None => Event::None,
                    ConnectionPost::Data(_,_) => Event::None,
                    ConnectionPost::Reading(_,_,_,_) => Event::Read,
                    ConnectionPost::Complete => Event::None,
                }
                
            },
            ConnectionMode::SendingResponse(_, _, _) => Event::Write,
        }
    }
    
    pub fn get_timer_mode(&self) -> TimerMode {
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(_, _) => TimerMode::In,
            
            ConnectionMode::WaitingForServerResponse(_, ref connection_post) => {
                
                match *connection_post {
                    ConnectionPost::None => TimerMode::None,
                    ConnectionPost::Data(_,_) => TimerMode::None,
                    ConnectionPost::Reading(_,_,_,_) => TimerMode::Post,
                    ConnectionPost::Complete => TimerMode::None,
                }
                
            },
            
            ConnectionMode::SendingResponse(_, _, _) => TimerMode::Out,
        }
    }
    
    pub fn get_name(&self) -> &str {
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(_, _) => "ReadingRequest",
            
            ConnectionMode::WaitingForServerResponse(_, ref connection_post) => {
                
                match *connection_post {
                    ConnectionPost::None => "WaitingForServerResponse (post none)",
                    ConnectionPost::Data(_,_) => "WaitingForServerResponse (post data)",
                    ConnectionPost::Reading(_,_,_,_) => "WaitingForServerResponse (post reading)",
                    ConnectionPost::Complete => "WaitingForServerResponse (post complete)",
                }
            },
            
            ConnectionMode::SendingResponse(_, _, _) => "SendingResponse"
        }
    }
    
    
    pub fn set_callback_post(self, request: Request, callback: Box<FnBox(Request, Option<Vec<u8>>) + Send + Sync + 'static>) -> (Result<Connection, TcpStream>, LogMessage) {
        
        match self.mode {
            
            ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::None) => {
                
                (Ok(Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Complete))), LogMessage::None)
            },
            
            ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Data(vector, len)) => {
                
                (Ok(Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Reading(vector, len, request, callback)))), LogMessage::None)
            },
                        
            _ => {
                (Err(self.stream), LogMessage::Error("Nieprawidłowy stan".to_owned()))
            }
        }
    }
    
    
    pub fn ready(mut self, events: EventSet, server_down: bool) -> (Result<Connection, TcpStream>, Option<PreRequest>, LogMessage) {
        
        if events.is_error() {
            
            let log_message = format!("ready error, {:?}", events);
            
            return (Err(self.stream), None, LogMessage::Error(log_message));
        }
        
        if events.is_hup() {
            
            let log_message = format!("ready, event hup, {:?}", events);
            
            return (Err(self.stream), None, LogMessage::Error(log_message));
        }
        
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(buf, done) => {

                transform_from_waiting_for_user(self.stream, events, buf, done)
            },
            
            ConnectionMode::WaitingForServerResponse(keep_alive, connection_post) => {
                
                if let ConnectionPost::Reading(mut vec, len, request, callback_post) = connection_post {
                    
                    
                    let mut buf : [u8; 2048] = [0; 2048];
                    
                    
                    loop {
                        
                        
                        if vec.len() == len {
                            break;
                        } else if vec.len() > len {
                            unreachable!();
                        }
                        
                        
                        let max_index = min(len - vec.len(), 2048);
                        
                        match self.stream.try_read(&mut buf[0..max_index]) {

                            Ok(Some(size)) => {
                                
                                if size > max_index {
                                    unreachable!();
                                }
                                
                                vec.extend_from_slice(&buf[0..size]);
                                
                                //to może być dobre miejsce na parsowanie tego kawałka który napłynął
                            },

                            Ok(None) => {

                                break;
                            },

                            Err(err) => {

                                let message = format!("error write to socket, {:?}", err);

                                let new_conn = Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Reading(vec, len, request, callback_post)));

                                return (Ok(new_conn), None, LogMessage::Error(message));
                            }
                        }
                    };
                    
                    
                    let connection_post = ConnectionPost::Reading(vec, len, request, callback_post);
                    
                    let new_conn = Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive, connection_post));

                    return (Ok(new_conn), None, LogMessage::None);
                }
                
                
                (Ok(Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive, connection_post))), None, LogMessage::None)
            },
            
            ConnectionMode::SendingResponse(keep_alive, str, done) => {

                let (new_conn, log_mess) = transform_from_sending_to_user(self.stream, keep_alive, events, str, done, server_down);
                
                (new_conn, None, log_mess)
            },
        }
    }
    
    
    
    pub fn check_post(self) -> Connection {
        
        
        if let ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Reading(vec, len, request, callback_post)) = self.mode {

            let new_mode = if vec.len() == len {
                
                (callback_post as Box<FnBox(Request, Option<Vec<u8>>)>)(request, Some(vec));
                ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Complete)

            } else {

                ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Reading(vec, len, request, callback_post))
            };
            
            return Connection::make(self.stream, new_mode);
        }
        
        self
    }

}

fn transform_from_waiting_for_user(mut stream: TcpStream, events: EventSet, mut buf: [u8; 2048], done: usize) -> (Result<Connection, TcpStream>, Option<PreRequest>, LogMessage) {

    if events.is_readable() {

        let total = buf.len();
        
        match stream.try_read(&mut buf[done..total]) {

            Ok(Some(size)) => {

                if size > 0 {

                    let done = done + size;
                    
                    let mut headers = [httparse::EMPTY_HEADER; 100];
                    let mut req     = httparse::Request::new(&mut headers);
                    
                    match req.parse(&buf[0..size]) {
                        
                        Ok(httparse::Status::Complete(size_parse)) => {
                            
                            match PreRequest::new(req) {

                                Ok(pre_request) => {
                                    
                                    
                                    let keep_alive = pre_request.is_header_set("Connection", "keep-alive");
                                    
                                    
                                    let connection_post = if pre_request.is_post() {
                                        
                                        if size > size_parse {
                                            
                                            let mut post_data = vec![];
                                            
                                            post_data.extend_from_slice(&buf[size_parse..size]);
                                            
                                            
                                            if let Some(req_len) = pre_request.get_header("Content-Length".to_owned()) {
                                                
                                                if req_len >= post_data.len() {
                                                    
                                                    ConnectionPost::Data(post_data, req_len)
                                                    
                                                } else {
                                                    
                                                    let response_400 = response::Response::create_400();
                                                    let new_connection = ConnectionMode::SendingResponse(false, response_400.as_bytes(), 0);
                                                    return (Ok(Connection::make(stream, new_connection)), None, LogMessage::None)
                                                }
                                                
                                            } else {
                                                
                                                let response_400 = response::Response::create_400();
                                                let new_connection = ConnectionMode::SendingResponse(false, response_400.as_bytes(), 0);
                                                return (Ok(Connection::make(stream, new_connection)), None, LogMessage::None)
                                            }
                                            
                                        } else {
                                            
                                            ConnectionPost::None
                                        }
                                        
                                    } else {
                                        
                                        ConnectionPost::None
                                    };
                                    
                                    
                                    
                                    (Ok(Connection::make(stream, ConnectionMode::WaitingForServerResponse(keep_alive, connection_post))), Some(pre_request), LogMessage::None)
                                }

                                Err(err) => {
                                    
                                    let log_mess = format!("error prepare request, {:?}", err);
                                    
                                    let response_400   = response::Response::create_400();
                                    let new_connection = ConnectionMode::SendingResponse(false, response_400.as_bytes(), 0);
                                    
                                    (Ok(Connection::make(stream, new_connection)), None, LogMessage::Error(log_mess))
                                }
                            }
                        }

                                                            //częściowe parsowanie
                        Ok(httparse::Status::Partial) => {
                            
                            if buf.len() == done {
                                
                                let response_400 = response::Response::create_400();
                                let new_connection = ConnectionMode::SendingResponse(false, response_400.as_bytes(), 0);
                                return (Ok(Connection::make(stream, new_connection)), None, LogMessage::None)
                            }
                            
                            (Ok(Connection::make(stream, ConnectionMode::ReadingRequest(buf, done))), None, LogMessage::None)
                        }

                        Err(err) => {
                            
                            match err {
                                
//TODO - zrobić formatowanie komunikatu z błędem -> wrzucać na strumień błędów
                                
                                httparse::Error::HeaderName => {
                                    println!("header name");
                                }
                                _ => {
                                    println!("error parse {:?}", err);
                                }
                            }

                            /* HeaderName, HeaderValue, NewLine, Status, Token, TooManyHeaders, Version */
                            
                            let response_400 = response::Response::create_400();
                            let new_connection = ConnectionMode::SendingResponse(false, response_400.as_bytes(), 0);
                            (Ok(Connection::make(stream, new_connection)), None, LogMessage::None)
                        }
                    }


                    //uruchom parser
                        //jeśli się udało sparsować, to git

                    //jeśli osiągneliśmy całkowity rozmiar bufora a mimo to nie udało się sparsować danych
                        //to rzuć błędem że nieprawidłowe zapytanie

                } else {

                    (Ok(Connection::make(stream, ConnectionMode::ReadingRequest(buf, done))), None, LogMessage::None)
                }
            }

            Ok(None) => {
                (Ok(Connection::make(stream, (ConnectionMode::ReadingRequest(buf, done)))), None, LogMessage::None)
            }

            Err(err) => {
                
                let message = format!("error read from socket, {:?}", err);
                
                (Ok(Connection::make(stream, (ConnectionMode::ReadingRequest(buf, done)))), None, LogMessage::Error(message))
            }
        }

    } else {
        
        (Ok(Connection::make(stream, ConnectionMode::ReadingRequest(buf, done))), None, LogMessage::None)
    }
}


fn transform_from_sending_to_user(mut stream: TcpStream, keep_alive: bool, events: EventSet, str: Vec<u8>, done: usize, server_down: bool) -> (Result<Connection, TcpStream>, LogMessage) {

    if events.is_writable() {

        match stream.try_write(&str[done..str.len()]) {

            Ok(Some(size)) => {
                
                if size > 0 {

                    let done = done + size;

                                                    //send all data to browser
                    if done == str.len() {

                                                    //keep connection
                        
                        if server_down == false && keep_alive == true {

                            let mess = format!("keep alive");
                            
                            let new_conn = Connection::make(stream, (ConnectionMode::ReadingRequest([0u8; 2048], 0)));
                            
                            return (Ok(new_conn), LogMessage::Message(mess));
                            
                                                    //close connection
                        } else {
                            
                            return (Err(stream), LogMessage::None);
                        }

                    } else if done < str.len() {
                        
                        let new_conn = Connection::make(stream, (ConnectionMode::SendingResponse(keep_alive, str, done)));
                        
                        return (Ok(new_conn), LogMessage::None);

                    } else {

                        unreachable!();
                    }

                } else {
                    
                    let new_conn = Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done));
                    
                    return (Ok(new_conn), LogMessage::None);
                }
            }

            Ok(None) => {

                let new_conn = Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done));
                
                (Ok(new_conn), LogMessage::None)
            }

            Err(err) => {

                let message = format!("error write to socket, {:?}", err);
                
                let new_conn = Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done));
                
                return (Ok(new_conn), LogMessage::Error(message));
            }
        }

    } else {

        let new_conn = Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done));
        
        (Ok(new_conn), LogMessage::None)
    }
}
