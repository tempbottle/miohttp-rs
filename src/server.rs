use std::collections::HashMap;
use std::io;
use mio::{Token, EventLoop, EventSet, PollOpt, Handler, Timeout};
use mio::tcp::{TcpListener, TcpStream};
//use mio::util::Slab;                 //TODO - użyć tego modułu zamiast hashmapy
use std::mem;
use response;
use connection::{Connection, TimerMode, LogMessage};
use token_gen::TokenGen;
use request::{PreRequest, Request};
use new_socket::new_socket;
use miostart::MioStart;
use miodown::MioDown;
use std::time::Duration;

use std::boxed::FnBox;

pub type FnReceiver   = Box<Fn(Request) + Send + Sync + 'static>;
pub type FnLog        = Box<Fn(bool, String) + Send + Sync + 'static>;
pub type TransformOut = (Result<Connection, TcpStream>, Option<PreRequest>, LogMessage);


// Define a handler to process the events
pub struct MyHandler {
    token           : Token,
    server          : Option<TcpListener>,                  //Some - serwer nasłuchuje, None - jest w trybie wyłączania
    hash            : HashMap<Token, (Connection, Event, Option<Timeout>)>,
    tokens          : TokenGen,
    timeout_reading : u64,
    timeout_writing : u64,
    fn_log          : Option<FnLog>,
    fn_receiver     : FnReceiver,
}


// Event type which is set for socket in event_loop
//#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
#[derive(PartialEq)]
pub enum Event {
    Init,
    Write,
    Read,
    None
}


pub enum MioMessage {
    Response(Token, response::Response),
    Down,
    GetPost(Token, Request, Box<FnBox(Request, Option<Vec<u8>>) + Send + Sync + 'static>),
}


pub fn new_server(addres: String, timeout_reading: u64, timeout_writing:u64, fn_log: Option<FnLog>, fn_receiver : FnReceiver) -> (MioStart, MioDown) {

    let mut event_loop = EventLoop::new().unwrap();

    let chan_shoutdown = event_loop.channel();

    let fn_start = Box::new(move ||{

        let server = new_socket(addres);

        let mut tokens = TokenGen::new();

        let token = tokens.get();

        event_loop.register(&server, token, EventSet::readable(), PollOpt::edge()).unwrap();

        let mut inst = MyHandler {
            token           : token,
            server          : Some(server),
            hash            : HashMap::new(),
            tokens          : tokens,
            timeout_reading : timeout_reading,
            timeout_writing : timeout_writing,
            fn_log          : fn_log,
            fn_receiver     : fn_receiver,
        };

        event_loop.run(&mut inst).unwrap();
    });

    (MioStart::new(fn_start), MioDown::new(chan_shoutdown), )
}


impl Handler for MyHandler {

    type Timeout = Token;
    type Message = MioMessage;
    
    fn ready(&mut self, event_loop: &mut EventLoop<MyHandler>, token: Token, events: EventSet) {
        
        self.log_mess(&token, format!("ready, {:?}", events));
        
        if token == self.token {
            
            self.new_connection(event_loop);
            
        } else {
            
            let server_down = self.server.is_none();

            self.transform_connection(event_loop, &token, move|connection_prev : Connection| -> TransformOut {
                
                connection_prev.ready(events, server_down)
            });
        }
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
        
        match msg {
            
            MioMessage::Response(token, response) => {
                
                self.transform_connection(event_loop, &token, move|connection_prev : Connection| -> TransformOut {

                    let (new_conn, log_message) = connection_prev.send_data_to_user(response);

                    (Ok(new_conn), None, log_message)
                });
            },
            
            MioMessage::Down => {
                
                match mem::replace(&mut self.server, None) {
                    
                    Some(server) => {

                        event_loop.deregister(&server).unwrap();
                        
                        self.test_close_mio(event_loop);
                        
                    },
                    None => {
                        
                        panic!("Powielony sygnał wyłączenia event_loop-a");
                    }
                };
            },
            
            MioMessage::GetPost(token, request, callback) => {
                
                self.transform_connection(event_loop, &token, move|connection_prev : Connection| -> TransformOut {
                    
                    let (conn, log_mess) = connection_prev.set_callback_post(request, callback);
                    
                    (conn, None, log_mess)
                });
            }
        };
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<Self>, token: Self::Timeout) {
        
        
        self.transform_connection(event_loop, &token, move|connection_prev : Connection| -> TransformOut {

            let (conn, mess) = connection_prev.timeout_trigger();

            (conn, None, mess)
        });
    }
}


impl MyHandler {
    
    fn log_error(&self, token: &Token, mess : String) {
        
        match self.fn_log {
            
            Some(ref fn_log) => {
                
                fn_log(true, format!("miohttp {} / {} -> {}", token.as_usize(), self.hash.len(), mess))
            },
            
            None => {}
        }
    }
    
    fn log_mess(&self, token: &Token, mess : String) {
        
        match self.fn_log {
            
            Some(ref fn_log) => {
                
                fn_log(false, format!("miohttp {} / {} -> {}", token.as_usize(), self.hash.len(), mess))
            },
            
            None => {}
        }
    }
    
    fn test_close_mio(&self, event_loop: &mut EventLoop<MyHandler>) {
        
        if self.server.is_none() && self.hash.len() == 0 {
            event_loop.shutdown();
        }
    }
    
    
    
    fn new_connection(&mut self, event_loop: &mut EventLoop<MyHandler>) {
        
        let new_connections = match &(self.server) {

            &Some(ref socket) => {
                
                self.get_new_connections(&socket)
            },
            
            &None => {
                
                self.log_mess(&self.token, "serwer znajduje się w trybie wyłączania".to_owned());
                Vec::new()
            }
        };
        
        for (addr, connection) in new_connections {
            
            let token = self.tokens.get();
            
            self.log_mess(&token, format!("new connection, addr = {}", addr));

            self.insert_connection(&token, connection, Event::Init, None, event_loop);
        }
    }
    
    
    fn get_new_connections(&self, socket: &TcpListener) -> Vec<(String, Connection)> {
        
        let mut list = Vec::new();
        
        loop {
            
            match socket.accept() {

                Ok(Some((stream, addr))) => {
                    list.push((format!("{}", addr), Connection::new(stream)));
                }
                
                Ok(None) => {
                    return list;
                }

                Err(err) => {

                    self.log_error(&self.token, format!("new connection err {}", err));
                    return list;
                }
            };
        }
    }
    


    fn set_event(&mut self, connection: &Connection, token: &Token, old_event: &Event, new_event: &Event, event_loop: &mut EventLoop<MyHandler>) -> Result<String, io::Error> {
        
        let pool_opt = PollOpt::edge() | PollOpt::oneshot();
        
        let new_mode = match *new_event {
            Event::Init  => None,
            Event::Write => Some(EventSet::error() | EventSet::hup() | EventSet::writable()),
            Event::Read  => Some(EventSet::error() | EventSet::hup() | EventSet::readable()),
            Event::None  => Some(EventSet::error() | EventSet::hup()),
        };
        
        if *old_event == Event::Init {
        
            match new_mode {
                None => Ok(format!("register: none")),
                Some(mode) => {
                    try!(event_loop.register(&connection.stream, token.clone(), mode, pool_opt));
                    Ok(format!("register: {:?}", mode))
                },
            }
        
        } else {
            
            match new_mode {
                None => Ok(format!("reregister: none")),
                Some(mode) => {
                    try!(event_loop.reregister(&connection.stream, token.clone(), mode, pool_opt));
                    Ok(format!("reregister: {:?}", mode))
                },
            }
        }
    }

    
    fn set_timer(&mut self, token: &Token, timeout: Option<Timeout>, timer_mode: TimerMode, event_loop: &mut EventLoop<MyHandler>) -> (Option<Timeout>, String) {
        
        match timeout {
            
            Some(timeout) => {
                
                match timer_mode {
                    
                    TimerMode::In   => (Some(timeout), "keep".to_owned()),
                    TimerMode::Out  => (Some(timeout), "keep".to_owned()),
                    TimerMode::Post => (Some(timeout), "keep".to_owned()),
                    TimerMode::None => {
                        let _ = event_loop.clear_timeout(&timeout);
                        (None, "clear".to_owned())
                    },
                }
            },
            
            None => {
                
                match timer_mode {
                    
                    TimerMode::In => {
                        
                        match event_loop.timeout(token.clone(), Duration::from_millis(self.timeout_reading)) {
                            
                            Ok(timeout) => (Some(timeout), "set IN".to_owned()),
                            Err(err)    => (None , format!("error IN {:?}", err)),
                        }
                            
                    },
                    
                    TimerMode::Out => {
                        
                        match event_loop.timeout(token.clone(), Duration::from_millis(self.timeout_writing)) {
                            
                            Ok(timeout) => (Some(timeout), "set OUT".to_owned()),
                            Err(err)    => (None , format!("error OUT {:?}", err)),
                        }
                    },
                    
                    TimerMode::Post => {
                                                                //TODO - ten parametr możnaby zrobić dedykowany
                        
                        match event_loop.timeout(token.clone(), Duration::from_millis(self.timeout_reading)) {
                            
                            Ok(timeout) => (Some(timeout), "set POST".to_owned()),
                            Err(err)    => (None , format!("error POST {:?}", err)),
                        }
                    },
                    
                    TimerMode::None => (None, "none".to_owned()),
                }
            },
        }
    }
    
    fn insert_connection(&mut self, token: &Token, connection: Connection, old_event: Event, timeout: Option<Timeout>, event_loop: &mut EventLoop<MyHandler>) {

        let new_event = connection.get_event();
        
        let mess_event = if old_event != new_event {
            match self.set_event(&connection, token, &old_event, &new_event, event_loop) {
                Ok(str) => str,
                Err(err) => {
                    self.log_error(token, format!("set_event: {}", err));
                    return;
                }
            }
        } else {
            "none".to_owned()
        };
        
        
        let (new_timer, timer_message) = self.set_timer(token, timeout, connection.get_timer_mode(), event_loop);
        
        
        self.log_mess(token, format!("set mode {}, {}, timer {}", connection.get_name(), mess_event, timer_message));
        
        self.hash.insert(token.clone(), (connection, new_event, new_timer));
    }
    
    fn transform_connection<F>(&mut self, event_loop: &mut EventLoop<MyHandler>, token: &Token, process: F)
        where F : FnOnce(Connection) -> (Result<Connection, TcpStream>, Option<PreRequest>, LogMessage) {
        
        let res = self.hash.remove(&token);
        
        match res {
            
            Some((connection_prev, old_event, timeout)) => {
                
                let (conenction_opt, request_opt, log_message) = process(connection_prev);
                
                
                match log_message {
                    LogMessage::Message(mess) => self.log_mess(token, mess),
                    LogMessage::Error(mess) => self.log_error(token, mess),
                    LogMessage::None => {},
                }
                
                
                match conenction_opt {
                    
                    Ok(connection_new) => {
                        
                                        //sprawdź czy dane z posta są kompletne
                        let connection_new = connection_new.check_post();
                        
                        self.insert_connection(&token, connection_new, old_event, timeout, event_loop);
                    },
                    
                    Err(stream) => {
                        
                        if let Some(ref timeout_value) = timeout {
                            let _ = event_loop.clear_timeout(timeout_value);
                        }
                        
                        event_loop.deregister(&stream).unwrap();
                        
                        self.log_mess(token, "close connection".to_owned());
                    }
                };
                
                
                
                if let Some(pre_request) = request_opt {
                    
                    let request  = pre_request.bind(token.clone(), event_loop.channel());

                    (self.fn_receiver)(request);
                }
            },
            
            None => {
                
                self.log_error(token, "no socket by token".to_owned());
            }
        };
        
        self.test_close_mio(event_loop);
    }
}

