use mio::{Token, Sender};
use response::Response;
use server::MioMessage;

pub struct Respchan {
    //is_send : bool,
    token   : Token,
    sender  : Sender<MioMessage>,
}


impl Respchan {
    
    pub fn new(token: Token, sender: Sender<MioMessage>) -> Respchan {
        
        Respchan {
            //is_send : false,
            token   : token,
            sender  : sender
        }
    }
    
    pub fn send(self, response: Response) {
        
        //self.is_send = true;
        
        (self.sender).send(MioMessage::Response(self.token, response)).unwrap();
    }
}

/*
&mut 
impl Drop for Respchan {

    fn drop(&mut self) {

        if self.is_send == false {
            
            panic!("unhandled request");
        }
    }
}
*/

