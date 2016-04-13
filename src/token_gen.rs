use mio::{Token};

pub struct TokenGen {
    count : usize
}

impl TokenGen {

    pub fn new() -> TokenGen {
        TokenGen{count : 0}
    }

    pub fn get(&mut self) -> Token {
        let curr = self.count;
        self.count = self.count + 1;

        Token(curr)
    }

}
