extern crate miohttp;

use miohttp::{new_server, Request, Response, Code, Type};
use std::thread;
use std::time::Duration;


fn main() {
    
    println!("server start: http://127.0.0.1:9876");
    
    
    
    let log_error = Box::new(|is_error : bool, message:String|{
        
        if is_error {
            
            println!("ERROR: {}", message);
            
        } else {
            
            println!("LOG  : {}", message);
        }
    });
    
    
    let addres = "127.0.0.1:9876".to_owned();
    
    let (miostart, miodown) = new_server(addres, 4000, 4000, Some(log_error), Box::new(move|request:Request| {
        
        let resp = Response::create(Code::Code200, Type::TextHtml, "Hello world -> ".to_owned() + request.path());
        
        request.send(resp);
    }));
    
    
    thread::spawn(move||{
        
        miostart.start();
    });
    
    
                //20 sekund
    thread::sleep(Duration::from_millis(20000));
    
    
    miodown.shoutdown();
}
