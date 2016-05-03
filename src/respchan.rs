use mio::{Token, Sender};
use response::Response;
use server::MioMessage;

pub struct Respchan {
    token   : Token,
    sender  : Sender<MioMessage>,
}


impl Respchan {
    
    pub fn new(token: Token, sender: Sender<MioMessage>) -> Respchan {
        
        Respchan {
            token   : token,
            sender  : sender
        }
    }
    
    pub fn send(self, response: Response) {
        
        (self.sender).send(MioMessage::Response(self.token, response)).unwrap();
    }
}
