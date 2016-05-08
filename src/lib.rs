#![feature(fnbox)]

extern crate mio;
extern crate httparse;
extern crate net2;
extern crate libc;

mod token_gen;
mod request;
mod response;
mod connection;
mod server;
mod new_socket;
mod miostart;
mod miodown;
mod typemod;
mod code;

pub use server::new_server;
pub use request::Request;
pub use response::Response;
pub use miostart::MioStart;
pub use miodown::MioDown;
pub use typemod::Type;
pub use code::Code;



#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
