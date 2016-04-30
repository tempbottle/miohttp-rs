use std::boxed::FnBox;

pub struct MioStart {
    fn_start : Box<FnBox() + Send + Sync + 'static>,
}

impl MioStart {
    
    pub fn new(fn_start: Box<FnBox() + Send + Sync + 'static>) -> MioStart {
        
        MioStart {
            fn_start : fn_start
        }
    }
    
    pub fn start(self) {
        
        (self.fn_start as Box<FnBox()>)();
        
        //(self.fn_start)();  // as Box<FnOnce()>)();
    }
}