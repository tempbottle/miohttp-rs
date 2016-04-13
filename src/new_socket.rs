use mio;
use std::os::unix::io::AsRawFd;
use net2;
use libc;
use mio::tcp::{TcpListener};

pub fn new_socket(addres: String) -> TcpListener {
    
    let sock = net2::TcpBuilder::new_v4().unwrap();
    let one = 1i32;
    unsafe {
        assert!(libc::setsockopt(
            sock.as_raw_fd(), libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &one as *const libc::c_int as *const libc::c_void, 4) == 0);
    }
    let addr = &addres.parse().unwrap();
    sock.bind(&addr).unwrap();
    let server = mio::tcp::TcpListener::from_listener(
        sock.listen(4096).unwrap(), &addr).unwrap();

    server
}