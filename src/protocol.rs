//--- Protocols -----
pub mod river_wm {
    pub extern crate wayland_client;
    pub use wayland_client::protocol::*;

    mod interfaces {
        pub(super) mod rwm {
            pub use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("./protocol/river-window-management-v1.xml");
        }
        pub(super) mod rxkb {
            use super::rwm::*;
            wayland_scanner::generate_interfaces!("./protocol/river-xkb-bindings-v1.xml");
        }
        pub(super) mod rls {
            use super::rwm::*;
            wayland_scanner::generate_interfaces!("./protocol/river-layer-shell-v1.xml");
        }
    }

    use self::interfaces::rwm::*;
    use self::interfaces::rxkb::*;
    use self::interfaces::rls::*;
    wayland_scanner::generate_client_code!("./protocol/river-window-management-v1.xml");
    wayland_scanner::generate_client_code!("./protocol/river-xkb-bindings-v1.xml");
    wayland_scanner::generate_client_code!("./protocol/river-layer-shell-v1.xml");
}

