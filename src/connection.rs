use mio::{Token, EventSet, TryRead, TryWrite};
use mio::tcp::{TcpStream};
use httparse;
use server::{Event};
use request::Request;
use response;
use task_async;

enum ConnectionMode {

                                                    //czytanie requestu
    ReadingRequest([u8; 2048], usize),
                                                    //oczekiwanie na wygenerowanie odpowiedzi serwera (bool to keep alive)
    WaitingForServerResponse(bool),
                                                    //wysyłanie odpowiedz (bool to keep alive)
    SendingResponse(bool, Vec<u8>, usize),
                                                    //połączenie do zamknięcia
    Close,
}


pub enum TimerMode {
    In,
    Out,
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

    fn replace_mode(self, mode: ConnectionMode) -> Connection {
        
        Connection {
            stream : self.stream,
            mode   : mode,
        }
    }

    pub fn in_state_close(&self) -> bool {

        match self.mode {
            ConnectionMode::Close => true,
            _ => false
        }
    }

    pub fn send_data_to_user(self, token: Token, response: response::Response) -> Connection {
        
        match self.mode {

            ConnectionMode::WaitingForServerResponse(keep_alive) => {
                
                let close_connection = response.close_connection();
                
                self.replace_mode(ConnectionMode::SendingResponse(keep_alive && close_connection == false, response.as_bytes(), 0))
            }

            _ => {
                
                task_async::log_error(format!("miohttp {} -> send_data_to_user: incorect state", token.as_usize()));
                self
            }
        }

        
    }

    pub fn get_event(&self) -> Event {

        match self.mode {
            
            ConnectionMode::ReadingRequest(_, _)     => Event::Read,
            ConnectionMode::SendingResponse(_, _, _) => Event::Write,
            _                                        => Event::None
        }
    }
    
    pub fn get_timer_mode(&self) -> TimerMode {
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(_, _)     => TimerMode::In,
            ConnectionMode::SendingResponse(_, _, _) => TimerMode::Out,
            _                                        => TimerMode::None
        }
    }
    
    pub fn get_name(&self) -> &str {
        
        match self.mode {
            
            ConnectionMode::ReadingRequest(_, _)        => "ReadingRequest",
            ConnectionMode::WaitingForServerResponse(_) => "WaitingForServerResponse",
            ConnectionMode::SendingResponse(_, _, _)    => "SendingResponse",
            ConnectionMode::Close                       => "Close"
        }
    }
    
    pub fn ready(self, events: EventSet, token: &Token, server_down: bool) -> (Connection, Option<Request>) {
        
        if events.is_error() {
            
            task_async::log_error(format!("miohttp {} -> ready error, {:?}", token.as_usize(), events));
            return (self.replace_mode(ConnectionMode::Close), None);
        }
        
        if events.is_hup() {
            
            task_async::log_info(format!("miohttp {} -> ready, event hup, {:?}", token.as_usize(), events));
            return (self.replace_mode(ConnectionMode::Close), None);
        }
        
        self.transform(events, token, server_down)
    }



    fn transform(self, events: EventSet, token: &Token, server_down: bool) -> (Connection, Option<Request>) {

        match self.mode {

            ConnectionMode::ReadingRequest(buf, done) => {

                transform_from_waiting_for_user(self.stream, events, buf, done, token)
            }

            ConnectionMode::WaitingForServerResponse(keep_alive) => {

                (Connection::make(self.stream, ConnectionMode::WaitingForServerResponse(keep_alive)), None)
            }
            
            ConnectionMode::SendingResponse(keep_alive, str, done) => {

                (transform_from_sending_to_user(self.stream, token, keep_alive, events, str, done, server_down), None)
            },

            ConnectionMode::Close => {

                (Connection::make(self.stream, ConnectionMode::Close), None)
            }
        }
    }
}

fn transform_from_waiting_for_user(mut stream: TcpStream, events: EventSet, mut buf: [u8; 2048], done: usize, token: &Token) -> (Connection, Option<Request>) {

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
                                    
                                    (Connection::make(stream, ConnectionMode::WaitingForServerResponse(keep_alive)), Some(request))
                                }

                                Err(err) => {
                                    
                                    task_async::log_error(format!("miohttp {} -> error prepare request, {:?}", token.as_usize(), err));
                                    
                                    let response_400 = response::Response::create_400();
                                    (Connection::make(stream, ConnectionMode::SendingResponse(false, response_400.as_bytes(), 0)), None)
                                }
                            }
                        }

                                                            //częściowe parsowanie
                        Ok(httparse::Status::Partial) => {

                            (Connection::make(stream, ConnectionMode::ReadingRequest(buf, done)), None)
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
                            (Connection::make(stream, ConnectionMode::SendingResponse(false, response_400.as_bytes(), 0)), None)
                        }
                    }


                    //uruchom parser
                        //jeśli się udało sparsować, to git

                    //jeśli osiągneliśmy całkowity rozmiar bufora a mimo to nie udało się sparsować danych
                        //to rzuć błędem że nieprawidłowe zapytanie

                } else {

                    (Connection::make(stream, (ConnectionMode::ReadingRequest(buf, done))), None)
                }
            }

            Ok(None) => {
                (Connection::make(stream, (ConnectionMode::ReadingRequest(buf, done))), None)
            }

            Err(err) => {
                
                task_async::log_error(format!("miohttp {} -> error read from socket, {:?}", token.as_usize(), err));
                
                (Connection::make(stream, (ConnectionMode::ReadingRequest(buf, done))), None)
            }
        }



        //czytaj, odczytane dane przekaż do parsera
        //jeśli otrzymalismy poprawny obiekt requestu to :
            // przełącz stan tego obiektu połączenia, na oczekiwanie na dane z serwera
            // wyślij kanałem odpowiednią informację o requescie
            // zwróć informację na zewnątrz tej funkcji że nic się nie dzieje z tym połaczeniem

    } else {

        //trzeba też ustawić jakiś timeout czekania na dane od użytkownika

        (Connection::make(stream, ConnectionMode::ReadingRequest(buf, done)), None)
    }
}


fn transform_from_sending_to_user(mut stream: TcpStream, token: &Token, keep_alive: bool, events: EventSet, str: Vec<u8>, done: usize, server_down: bool) -> Connection {

    if events.is_writable() {

        match stream.try_write(&str[done..str.len()]) {

            Ok(Some(size)) => {
                
                if size > 0 {

                    let done = done + size;

                                                    //send all data to browser
                    if done == str.len() {

                                                    //keep connection
                        
                        if server_down == false && keep_alive == true {

                            task_async::log_debug(format!("miohttp {} -> keep alive", token.as_usize()));
                            
                            return Connection::make(stream, (ConnectionMode::ReadingRequest([0u8; 2048], 0)));

                                                    //close connection
                        } else {
                            return Connection::make(stream, (ConnectionMode::Close));
                        }

                    } else if done < str.len() {

                        return Connection::make(stream, (ConnectionMode::SendingResponse(keep_alive, str, done)));

                    } else {

                        unreachable!();
                    }

                } else {

                    return Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done));
                }
            }

            Ok(None) => {

                Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done))
            }

            Err(err) => {

                task_async::log_error(format!("miohttp {} -> error write to socket, {:?}", token.as_usize(), err));
                Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done))
            }
        }

    } else {

        Connection::make(stream, ConnectionMode::SendingResponse(keep_alive, str, done))
    }
}
