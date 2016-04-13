use mio::Sender;
use server::MioMessage;

pub struct MioDown {
    chan : Sender<MioMessage>,
}

impl MioDown {
    
    pub fn new(chan: Sender<MioMessage>) -> MioDown {
        
        MioDown {
            chan : chan
        }
    }
    
    pub fn shoutdown(self) {
        
        self.chan.send(MioMessage::Down).unwrap();
    }
}