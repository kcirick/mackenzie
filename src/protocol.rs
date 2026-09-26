//--- Protocols -----
pub mod river_wm {
    pub extern crate wayland_client;
    pub use wayland_client::protocol::*;

    mod interfaces {
        pub(super) mod rwm {
            pub use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("./protocol/river-window-management-v1.xml");
        }
        pub(super) mod rxkbb {
            use super::rwm::*;
            wayland_scanner::generate_interfaces!("./protocol/river-xkb-bindings-v1.xml");
        }
        pub(super) mod rls {
            use super::rwm::*;
            wayland_scanner::generate_interfaces!("./protocol/river-layer-shell-v1.xml");
        }
        pub(super) mod rim {
            use super::rwm::*;
            wayland_scanner::generate_interfaces!("./protocol/river-input-management-v1.xml");
        }
        pub(super) mod rxkbc {
            use super::rim::*;
            wayland_scanner::generate_interfaces!("./protocol/river-xkb-config-v1.xml");
        }
        pub(super) mod rlc {
            use super::rim::*;
            wayland_scanner::generate_interfaces!("./protocol/river-libinput-config-v1.xml");
        }
    }

    use self::interfaces::rwm::*;
    use self::interfaces::rxkbb::*;
    use self::interfaces::rxkbc::*;
    use self::interfaces::rls::*;
    use self::interfaces::rim::*;
    use self::interfaces::rlc::*;
    wayland_scanner::generate_client_code!("./protocol/river-window-management-v1.xml");
    wayland_scanner::generate_client_code!("./protocol/river-xkb-bindings-v1.xml");
    wayland_scanner::generate_client_code!("./protocol/river-xkb-config-v1.xml");
    wayland_scanner::generate_client_code!("./protocol/river-layer-shell-v1.xml");
    wayland_scanner::generate_client_code!("./protocol/river-input-management-v1.xml");
    wayland_scanner::generate_client_code!("./protocol/river-libinput-config-v1.xml");
}

