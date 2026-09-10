pub mod config;
pub mod protocol;
pub mod wmcore;
pub mod output;
pub mod seat;
pub mod actions;
pub mod layout;
pub mod ipc;

use crate::config::Config;
use crate::wmcore::WMState;

use crate::ipc::ipc_get;
use crate::ipc::ipc_watch;
use crate::ipc::ipc_action;

use wayland_client::Connection;

use std::fs;
use std::os::unix::net::{UnixListener};
use std::os::unix::io::{AsFd,AsRawFd};

const SOCKET_PATH: &str = "/tmp/mackenzie.sock";

//--- Main function -----
fn main() -> Result<(), Box<dyn std::error::Error>> {

    // parse argument
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--version" => {
                println!("mackenzie version v0.1");
                return Ok(());
            }
            "--get" => {
                ipc_get(SOCKET_PATH, args[2].as_str());
                return Ok(());
            }
            "--watch" => {
                ipc_watch(SOCKET_PATH, args[2].as_str());
                return Ok(());
            }
            "--action" => {
                let action_args = args[2..].join(" ");
                ipc_action(SOCKET_PATH, action_args.as_str());
                return Ok(());
            }
            _ => {
                eprintln!("Error: Unknown argument '{}'", args[1]);
                std::process::exit(1);
            }
        }
    }

    // Load config
    let config = Config::load_config();

    // Queue up a get-registry event
    let conn = Connection::connect_to_env()?;
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let _registry = display.get_registry(&event_queue.handle(), ());

    let _ = fs::remove_file(&SOCKET_PATH);
    let listener = UnixListener::bind(&SOCKET_PATH).expect("Unable to create IPC Socket");
    listener.set_nonblocking(true).expect("Unable to set Socket non-blocking");

    let wayland_fd = conn.as_fd().as_raw_fd();
    let ipc_fd = listener.as_raw_fd();

    // Initial state
    let mut wmstate = WMState::new();
    wmstate.config = config;
    wmstate.ipc_listener = Some(listener);
    
    // Round trip to process the get_registry event and bind interfaces
    event_queue.roundtrip(&mut wmstate)?;
    if wmstate.river_wm.is_none() {
        eprintln!("river_window_manager_v1 global not found! Is river running?");
        std::process::exit(1);
    }

    // Main loop
    loop {
        // See https://docs.rs/wayland-client/latest/wayland_client/struct.EventQueue.html 
        // Step A: Send any pending outgoing requests to the compositor
        let _ = event_queue.flush();

        // Step B: Dispatch any events already sitting in the internal buffer
        let read_guard = match event_queue.prepare_read() {
            Some(guard) => guard,
            None => {
                event_queue.dispatch_pending(&mut wmstate).unwrap();
                continue;
            }
        };

        // include epoll(..) to wait for readiness of FD
        let mut fds = vec![
            libc::pollfd {
                fd: wayland_fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: ipc_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];

        // Step D: Execute the raw libc system call
        // --> Timeout is set to -1 to block indefinitely until an event occurs
        let poll_ret = unsafe { 
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) 
        };

        // Step E: Check if events are ready for reading
        if poll_ret>0 {
            if fds[0].revents & libc::POLLIN != 0 {
                read_guard.read().unwrap();
                event_queue.dispatch_pending(&mut wmstate).unwrap();
            } else {
                std::mem::drop(read_guard);
            }
            
            if fds[1].revents & libc::POLLIN != 0 {
                println!("IPC POLLIN");
                wmstate.handle_ipc_connections();
            }
        } else {
            std::mem::drop(read_guard);
        }

        // Step F: Broadcast if data changed during this iteration
        if wmstate.ipc_update_requested {
            wmstate.ipc_broadcast();
            wmstate.ipc_update_requested = false;
        }
    }
}
