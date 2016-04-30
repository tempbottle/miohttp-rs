use std::collections::HashMap;
use std::io;
use mio::{Token, EventLoop, EventSet, PollOpt, Handler, Timeout};
use mio::tcp::{TcpListener, TcpStream};
//use mio::util::Slab;                 //TODO - użyć tego modułu zamiast hashmapy
use std::mem;
use response;
use connection::{Connection, TimerMode};
use token_gen::TokenGen;
use request::Request;
use respchan::Respchan;
use new_socket::new_socket;
use miostart::MioStart;
use miodown::MioDown;
use task_async;
use std::time::Duration;


pub type FnReceiver = Box<Fn((Request, Respchan)) + Send + Sync + 'static>;


// Define a handler to process the events
pub struct MyHandler {
    token           : Token,
    server          : Option<TcpListener>,                  //Some - serwer nasłuchuje, None - jest w trybie wyłączania
    hash            : HashMap<Token, (Connection, Event, Option<Timeout>)>,
    tokens          : TokenGen,
    timeout_reading : u64,
    timeout_writing : u64,
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
}


pub fn new_server(addres: String, timeout_reading: u64, timeout_writing:u64, fn_receiver : FnReceiver) -> (MioStart, MioDown) {

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

        task_async::log_debug(format!("miohttp {} -> ready, {:?} (is server = {})", token.as_usize(), events, token == self.token));

        if token == self.token {
            self.new_connection(event_loop);
        } else {
            self.socket_ready(event_loop, &token, events);
        }
        
        self.test_close_mio(event_loop);
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Self>, msg: Self::Message) {
        
        match msg {
            
            MioMessage::Response(token, response) => {
                self.send_data_to_user(event_loop, token, response);
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
            }
        };
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<Self>, token: Self::Timeout) {
        
        self.timeout_trigger(&token, event_loop);
        
        self.test_close_mio(event_loop);
    }
}


impl MyHandler {
    
    fn test_close_mio(&self, event_loop: &mut EventLoop<MyHandler>) {
        
        if self.server.is_none() && self.hash.len() == 0 {
            event_loop.shutdown();
        }
    }
    
    fn send_data_to_user(&mut self, event_loop: &mut EventLoop<MyHandler>, token: Token, response: response::Response) {
        
        self.transform_connection(event_loop, &token, move|connection_prev : Connection| -> (Result<Connection, TcpStream>, Option<Request>) {

            (Ok(connection_prev.send_data_to_user(token.clone(), response)), None)
        });
    }
    
    
    fn timeout_trigger(&mut self, token: &Token, event_loop: &mut EventLoop<MyHandler>) {
        
        let token = token.clone();
        
        self.transform_connection(event_loop, &token, move|connection_prev : Connection| -> (Result<Connection, TcpStream>, Option<Request>) {
            
            task_async::log_debug(format!("miohttp {} -> timeout_trigger ok", token.as_usize()));
            (Err(connection_prev.get_stream()), None)
        });
    }
    
    
    fn new_connection(&mut self, event_loop: &mut EventLoop<MyHandler>) {
        
        let new_connections = match &(self.server) {

            &Some(ref socket) => {
                
                self.get_new_connections(&socket)
            },
            
            &None => {
                task_async::log_info(format!("serwer znajduje się w trybie wyłączania"));
                Vec::new()
            }
        };
        
        for (addr, connection) in new_connections {
            
            let token = self.tokens.get();

            task_async::log_info(format!("miohttp {} -> new connection, addr = {}", token.as_usize(), addr));

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

                    task_async::log_error(format!("miohttp {} -> new connection err {}", self.token.as_usize(), err));
                    return list;
                }
            };
        }
    }
    
    
    fn socket_ready(&mut self, event_loop: &mut EventLoop<MyHandler>, token: &Token, events: EventSet) {
        
        let token       = token.clone();
        let server_down = self.server.is_none();
        
        let request_opt = self.transform_connection(event_loop, &token, move|connection_prev : Connection| -> (Result<Connection, TcpStream>, Option<Request>) {

            let (connection_opt, request_opt) = connection_prev.ready(events, &token, server_down);

            match connection_opt {

                Ok(connection) => {

                    match request_opt {

                        Some(request) => {
                            
                            (Ok(connection), Some(request))
                        }

                        None => {
                            (Ok(connection), None)
                        }
                    }
                },

                Err(stream) => {
                    
                    (Err(stream), None)
                }
            }
        });
        
        if let Some(request) = request_opt {
            
            let respchan = Respchan::new(token.clone(), event_loop.channel());
            
            (self.fn_receiver)((request, respchan));
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
                    
                    TimerMode::In  => (Some(timeout), "keep".to_owned()),
                    TimerMode::Out => (Some(timeout), "keep".to_owned()),
                    
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
                        
                        //match event_loop.timeout_ms(token.clone(), self.timeout_reading) {
                            
                            Ok(timeout) => (Some(timeout), "timer in set".to_owned()),
                            Err(err)    => (None , format!("timer in error {:?}", err)),
                        }
                            
                    },
                    
                    TimerMode::Out => {
                        
                        match event_loop.timeout(token.clone(), Duration::from_millis(self.timeout_writing)) {
                        //match event_loop.timeout_ms(token.clone(), self.timeout_writing) {
                            
                            Ok(timeout) => (Some(timeout), "timer out set".to_owned()),
                            Err(err)    => (None , format!("timer out error {:?}", err)),
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
                    task_async::log_error(format!("set_event: {}", err));
                    return;
                }
            }
        } else {
            "none".to_owned()
        };
        
        
        let (new_timer, timer_message) = self.set_timer(token, timeout, connection.get_timer_mode(), event_loop);
        
        
        task_async::log_debug(format!("miohttp {} -> set mode {}, {}, timer {}", token.as_usize(), connection.get_name(), mess_event, timer_message));
        
        self.hash.insert(token.clone(), (connection, new_event, new_timer));
        
        task_async::log_debug(format!("count hasmapy after insert {}", self.hash.len()));
    }
    
    fn transform_connection<F>(&mut self, event_loop: &mut EventLoop<MyHandler>, token: &Token, process: F) -> Option<Request>
        where F : FnOnce(Connection) -> (Result<Connection, TcpStream>, Option<Request>) {
        
        let res = self.hash.remove(&token);
        
        task_async::log_debug(format!("hashmap after decrement {}", self.hash.len()));
        
        match res {
            
            Some((connection_prev, old_event, timeout)) => {
                
                let (conenction_opt, request_opt) = process(connection_prev);
                
                match conenction_opt {
                    
                    Ok(connection_new) => {
                        
                        self.insert_connection(&token, connection_new, old_event, timeout, event_loop);
                    },
                    
                    Err(stream) => {
                        
                        if let Some(ref timeout_value) = timeout {
                            let _ = event_loop.clear_timeout(timeout_value);
                        }
                        
                        event_loop.deregister(&stream).unwrap();
                    }
                };
                
                request_opt
                
            },
            
            None => {
                
                task_async::log_info(format!("miohttp {} -> no socket by token", token.as_usize()));
                
                None
            }
        }
    }


}

