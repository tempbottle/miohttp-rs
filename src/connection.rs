use mio::{Token, EventSet, TryRead, TryWrite};
use mio::tcp::{TcpStream};
use httparse;
use server::{Event};
use request::Request;
use response;

enum ConnectionMode {

                                                    //czytanie requestu
    ReadingRequest([u8; 2048], usize),
                                                    //oczekiwanie na wygenerowanie odpowiedzi serwera (bool to keep alive)
    WaitingForServerResponse(bool),
                                                    //wysyłanie odpowiedz (bool to keep alive)
    SendingResponse(bool, Vec<u8>, usize),
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

            ConnectionMode::WaitingForServerResponse(keep_alive) => {
                
                let new_keep_alive = if response.close_connection() {
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
            ConnectionMode::ReadingRequest(_, _)        => Event::Read,
            ConnectionMode::WaitingForServerResponse(_) => Event::None,
            ConnectionMode::SendingResponse(_, _, _)    => Event::Write,
        }
    }
    
    pub fn get_timer_mode(&self) -> TimerMode {
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(_, _)        => TimerMode::In,
            ConnectionMode::WaitingForServerResponse(_) => TimerMode::None,
            ConnectionMode::SendingResponse(_, _, _)    => TimerMode::Out,
        }
    }
    
    pub fn get_name(&self) -> &str {
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(_, _)        => "ReadingRequest",
            ConnectionMode::WaitingForServerResponse(_) => "WaitingForServerResponse",
            ConnectionMode::SendingResponse(_, _, _)    => "SendingResponse"
        }
    }
    
    pub fn ready(self, events: EventSet, token: &Token, server_down: bool) -> (Result<Connection, TcpStream>, Option<Request>, LogMessage) {
        
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

                transform_from_waiting_for_user(self.stream, events, buf, done, token)
            }
            
            ConnectionMode::WaitingForServerResponse(keep_alive) => {

                (Ok(Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive))), None, LogMessage::None)
            }
            
            ConnectionMode::SendingResponse(keep_alive, str, done) => {

                let (new_conn, log_mess) = transform_from_sending_to_user(self.stream, token, keep_alive, events, str, done, server_down);
                
                (new_conn, None, log_mess)
            }
        }
    }
}

fn transform_from_waiting_for_user(mut stream: TcpStream, events: EventSet, mut buf: [u8; 2048], done: usize, token: &Token) -> (Result<Connection, TcpStream>, Option<Request>, LogMessage) {

    if events.is_readable() {

        let total = buf.len();
        
        match stream.try_read(&mut buf[done..total]) {

            Ok(Some(size)) => {

                if size > 0 {

                    let done = done + size;
                    
                    let mut headers = [httparse::EMPTY_HEADER; 100];
                    let mut req     = httparse::Request::new(&mut headers);

                    match req.parse(&buf) {

                        Ok(httparse::Status::Complete(_)) => {      /*size_parse*/
                            
                            match Request::new(req) {

                                Ok(request) => {
                                    
                                    let keep_alive = request.is_header_set("Connection", "keep-alive");
                                    
                                    (Ok(Connection::make(stream, ConnectionMode::WaitingForServerResponse(keep_alive))), Some(request), LogMessage::None)
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



        //czytaj, odczytane dane przekaż do parsera
        //jeśli otrzymalismy poprawny obiekt requestu to :
            // przełącz stan tego obiektu połączenia, na oczekiwanie na dane z serwera
            // wyślij kanałem odpowiednią informację o requescie
            // zwróć informację na zewnątrz tej funkcji że nic się nie dzieje z tym połaczeniem

    } else {

        //trzeba też ustawić jakiś timeout czekania na dane od użytkownika

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
