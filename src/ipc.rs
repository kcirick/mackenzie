use std::io::{Read,Write};
use std::os::unix::net::UnixStream;
 
#[derive(PartialEq)]
enum IPCType {
    Get,
    Watch,
    Action,
}

pub fn ipc_get(socket: &str, arg: &str) {
    match arg {
        "tags" => ipc_tags(socket, IPCType::Get),
        "status" => ipc_status(socket, IPCType::Get),
        _ => {
            eprintln!("Error: unknown argument '{}'", arg);
        }
    }
}

pub fn ipc_watch(socket: &str, arg: &str) {
    match arg {
        "tags" => ipc_tags(socket, IPCType::Watch),
        _ => {
            eprintln!("Error: unknown argument '{}'", arg);
        }
    }
}

pub fn ipc_action(socket: &str, arg: &str) {
    let _ = IPCType::Action;
    if let Ok(mut stream) = UnixStream::connect(socket) {
        let _ = stream.write_all(arg.as_bytes());

        let mut buffer = [0; 128];
        let bytes_read = stream.read(&mut buffer).expect("Failed to read");
        let response = String::from_utf8_lossy(&buffer[..bytes_read]);
        println!("[Client] received response: {}", response);
    } else {
        eprintln!("Error connecting to IPC socket");
        std::process::exit(1);
    }
}

fn ipc_tags(socket: &str, ipc_type: IPCType) {
    if let Ok(mut stream) = UnixStream::connect(socket) {

        let mut buffer = [0; 1024];
        while let Ok(bytes_read) = stream.read(&mut buffer) {
            let response = String::from_utf8_lossy(&buffer[..bytes_read]);
            print!("{}\n", response);
            if ipc_type == IPCType::Get { break; }
        }
    } else {
        eprintln!("Error connecting to IPC socket");
        std::process::exit(1);
    }
}

fn ipc_status(socket: &str, ipc_type: IPCType) {
    if ipc_type != IPCType::Get { return; }

    if let Ok(_) = UnixStream::connect(socket) {
        println!("mackenzie is running");
    } else {
        eprintln!("Error connecting to IPC socket");
        std::process::exit(1);
    }
}

