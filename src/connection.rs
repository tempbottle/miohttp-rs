use mio::{Token, Sender, EventSet, TryRead, TryWrite};
use mio::tcp::{TcpStream};
use httparse;
use server::{Event, MioMessage};
use request::Request;
use response;
use std::cmp::min;

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
    Reading(Vec<u8>, usize, Box<Fn(Vec<u8>)>),
    Complete,
}

pub enum TimerMode {
    In,
    Out,
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
    
    pub fn get_stream(self) -> TcpStream {
        self.stream
    }
    
    pub fn send_data_to_user(self, token: Token, response: response::Response) -> (Connection, LogMessage) {
        
        match self.mode {

            ConnectionMode::WaitingForServerResponse(keep_alive, connection_post) => {
                
                let connection_post_close = match connection_post {
                    
                    ConnectionPost::None => false,
                    ConnectionPost::Data(_, _) => true,         //nieodebrane dane z posta, zamknij połączenie
                    ConnectionPost::Reading(_,_,_) => {
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
                
                let mess = format!("miohttp {} -> send_data_to_user: incorect state", token.as_usize());
                
                (self, LogMessage::Error(mess))
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
                    ConnectionPost::Reading(_,_,_) => Event::Read,
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
                    ConnectionPost::Reading(_,_,_) => TimerMode::In,
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
                    ConnectionPost::Reading(_,_,_) => "WaitingForServerResponse (post reading)",
                    ConnectionPost::Complete => "WaitingForServerResponse (post complete)",
                }
            },
            
            ConnectionMode::SendingResponse(_, _, _) => "SendingResponse"
        }
    }
    
    pub fn ready(mut self, events: EventSet, token: &Token, sender: Sender<MioMessage>, server_down: bool) -> (Result<Connection, TcpStream>, Option<Request>, LogMessage) {
        
        if events.is_error() {
            
            let log_message = format!("miohttp {} -> ready error, {:?}", token.as_usize(), events);
            
            return (Err(self.get_stream()), None, LogMessage::Error(log_message));
        }
        
        if events.is_hup() {
            
            let log_message = format!("miohttp {} -> ready, event hup, {:?}", token.as_usize(), events);
            
            return (Err(self.get_stream()), None, LogMessage::Error(log_message));
        }
        
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(buf, done) => {

                transform_from_waiting_for_user(self.stream, events, buf, done, token, sender)
            },
            
            ConnectionMode::WaitingForServerResponse(keep_alive, connection_post) => {
                
                if let ConnectionPost::Reading(mut vec, len, callback_post) = connection_post {
                    
                    
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

                                let message = format!("miohttp {} -> error write to socket, {:?}", token.as_usize(), err);

                                let new_conn = Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive, ConnectionPost::Reading(vec, len, callback_post)));

                                return (Ok(new_conn), None, LogMessage::Error(message));
                            }
                        }
                    };
                    
                    
                    let connection_post = if vec.len() == len {
                        
                        callback_post(vec);
                        ConnectionPost::Complete
                        
                    } else {
                        
                        ConnectionPost::Reading(vec, len, callback_post)
                    };
                    
                    
                    let new_conn = Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive, connection_post));

                    return (Ok(new_conn), None, LogMessage::None);
                }
                
                
                (Ok(Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive, connection_post))), None, LogMessage::None)
            },
            
            ConnectionMode::SendingResponse(keep_alive, str, done) => {

                let (new_conn, log_mess) = transform_from_sending_to_user(self.stream, token, keep_alive, events, str, done, server_down);
                
                (new_conn, None, log_mess)
            },
        }
    }
}

fn transform_from_waiting_for_user(mut stream: TcpStream, events: EventSet, mut buf: [u8; 2048], done: usize, token: &Token, sender: Sender<MioMessage>) -> (Result<Connection, TcpStream>, Option<Request>, LogMessage) {

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
                            
                            match Request::new(req, token.clone(), sender) {

                                Ok(request) => {
                                    
                                    
                                    let keep_alive = request.is_header_set("Connection", "keep-alive");
                                    
                                    
                                    let connection_post = if request.is_post() {
                                        
                                        if size > size_parse {
                                            
                                            let mut post_data = vec![];
                                            
                                            post_data.extend_from_slice(&buf[size_parse..size]);
                                            
                                            
                                            if let Some(req_len) = request.get_header("Content-Length".to_owned()) {
                                                
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
                                    
                                    
                                    
                                    (Ok(Connection::make(stream, ConnectionMode::WaitingForServerResponse(keep_alive, connection_post))), Some(request), LogMessage::None)
                                }

                                Err(err) => {
                                    
                                    let log_mess = format!("miohttp {} -> error prepare request, {:?}", token.as_usize(), err);
                                    
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
                
                let message = format!("miohttp {} -> error read from socket, {:?}", token.as_usize(), err);
                
                (Ok(Connection::make(stream, (ConnectionMode::ReadingRequest(buf, done)))), None, LogMessage::Error(message))
            }
        }

    } else {
        
        (Ok(Connection::make(stream, ConnectionMode::ReadingRequest(buf, done))), None, LogMessage::None)
    }
}


fn transform_from_sending_to_user(mut stream: TcpStream, token: &Token, keep_alive: bool, events: EventSet, str: Vec<u8>, done: usize, server_down: bool) -> (Result<Connection, TcpStream>, LogMessage) {

    if events.is_writable() {

        match stream.try_write(&str[done..str.len()]) {

            Ok(Some(size)) => {
                
                if size > 0 {

                    let done = done + size;

                                                    //send all data to browser
                    if done == str.len() {

                                                    //keep connection
                        
                        if server_down == false && keep_alive == true {

                            let mess = format!("miohttp {} -> keep alive", token.as_usize());
                            
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

                let message = format!("miohttp {} -> error write to socket, {:?}", token.as_usize(), err);
                
                let new_conn = Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done));
                
                return (Ok(new_conn), LogMessage::Error(message));
            }
        }

    } else {

        let new_conn = Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done));
        
        (Ok(new_conn), LogMessage::None)
    }
}
