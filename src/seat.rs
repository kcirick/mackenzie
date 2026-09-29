use std::collections::HashMap;
use xkbcommon::xkb;

use crate::protocol::river_wm::{
    river_window_v1::{Edges,RiverWindowV1},
    river_seat_v1::{Modifiers, RiverSeatV1},
    river_pointer_binding_v1::RiverPointerBindingV1,
    river_xkb_binding_v1::RiverXkbBindingV1,
    river_xkb_bindings_v1::RiverXkbBindingsV1,
    river_xkb_config_v1::{KeymapFormat, RiverXkbConfigV1},
    river_xkb_keyboard_v1::RiverXkbKeyboardV1,
    river_xkb_keymap_v1::RiverXkbKeymapV1,
    river_layer_shell_seat_v1::RiverLayerShellSeatV1,
    river_input_manager_v1::RiverInputManagerV1,
    river_input_device_v1::{Type as DeviceType, RiverInputDeviceV1},
    river_libinput_config_v1::RiverLibinputConfigV1,
    river_libinput_device_v1::RiverLibinputDeviceV1,
    river_libinput_result_v1::RiverLibinputResultV1,
};

use crate::actions::Action;
use crate::config::Config;
use crate::config::load_default_keybinds;
use crate::wmcore::WMState;
use crate::wmcore::Window;

use std::io::Write;
use tempfile::tempfile;
use std::os::unix::io::AsFd;

use wayland_backend::client::ObjectId;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

//--- Enums -----
#[derive(Debug, Clone)]
pub enum SeatOp {
    None,
    Move {
        window_proxy: RiverWindowV1,
        start_x: i32,
        start_y: i32,
    },
    Resize {
        window_proxy: RiverWindowV1,
        start_x: i32,
        start_y: i32,
        start_w: i32,
        start_h: i32,
        edges: Edges,
    },
}


//--- Helper functions -----
fn parse_modmask(key_string: &str) -> (Modifiers, String) {
    
    //let key_string_lowered = key_string.to_lowercase();
    let key_parts: Vec<&str> = key_string.split('-').collect();
    let mut modmask = Modifiers::empty();
    let mut remaining_key: String = String::new();
    for (i, p) in key_parts.iter().enumerate() {
        match p.to_lowercase().trim() {
            "shift"             => modmask |= Modifiers::Shift,
            "ctrl"              => modmask |= Modifiers::Ctrl,
            "alt" | "mod1"      => modmask |= Modifiers::Mod1,
            "super" | "mod4"    => modmask |= Modifiers::Mod4,
            _ => { }
        }
        if i+1 == key_parts.len() { remaining_key = p.to_string(); }
    }
    return (modmask, remaining_key)
} 

//--- Structs -----
#[derive(Debug)]
pub struct Seat {
    pub proxy: RiverSeatV1,
    pub hovered: Option<RiverWindowV1>,
    pub interacted: Option<RiverWindowV1>,
    pub xkb_bindings: HashMap<ObjectId, XkbBinding>,
    pub pointer_bindings: HashMap<ObjectId, PointerBinding>,

    pub pending_action: Action,
    pub op: SeatOp,
    cursor_x: i32,
    cursor_y: i32,
    pub op_dx: i32,
    pub op_dy: i32,
    pub op_release: bool,

    pub ignore_pointer_enter_event: bool,

    pub ls_seat: Option<RiverLayerShellSeatV1>,
}

#[derive(Debug)]
pub struct DeviceInfo {
    name: String,
    device_type: Option<DeviceType>,
    libinput_device: Option<RiverLibinputDeviceV1>,
    //supports_tap: bool,
}

#[derive(Debug)]
pub struct XkbBinding {
    pub proxy: RiverXkbBindingV1,
    action: Action,
}

#[derive(Debug)]
pub struct PointerBinding {
    pub proxy: RiverPointerBindingV1,
    action: Action,
}

//--- Implementation -----
impl Seat {
    pub fn new(proxy: RiverSeatV1) -> Self {
        Self {
            proxy,
            hovered: None,
            interacted: None,
            xkb_bindings: HashMap::new(),
            pointer_bindings: HashMap::new(),
            
            pending_action: Action::None,
            op: SeatOp::None,
            cursor_x: 0,
            cursor_y: 0,
            op_dx: 0,
            op_dy: 0,
            op_release: false,
            ignore_pointer_enter_event: false,
            ls_seat: None,
        }
    }

    pub fn keybinds_from_config (
        &mut self,
        river_xkb: &RiverXkbBindingsV1,
        config: &Config,
        qh: &QueueHandle<WMState>
    ) {
        if let Some(entries) = config.keybinds.clone() {
            for(key_string, action) in &entries {
                let (modmask, remaining_key) = parse_modmask(key_string);
                let keysym = xkb::keysym_from_name(remaining_key.as_str(), xkb::KEYSYM_NO_FLAGS);
                let my_action = Action::action_from_name(action.action.clone(), &action.args);

                let proxy = river_xkb.get_xkb_binding(&self.proxy, keysym.raw(), modmask, qh, self.proxy.id());
                proxy.enable();
                let binding = XkbBinding{ proxy, action: my_action };
                self.xkb_bindings.insert(binding.proxy.id(), binding);
            }
        } else {
            println!(" ---> No keybinds found. Defaulting to minimal set");
            let default_binds = load_default_keybinds();
            for bind in default_binds {
                let keysym = xkb::keysym_from_name(bind.key.as_str(), xkb::KEYSYM_NO_FLAGS);
                let proxy = river_xkb.get_xkb_binding(&self.proxy, keysym.raw(), bind.modmask, qh, self.proxy.id());
                proxy.enable();
                let binding = XkbBinding { proxy, action: bind.action };
                self.xkb_bindings.insert(binding.proxy.id(), binding);
            }
        }
    }

    pub fn mousebinds_from_config(
        &mut self, 
        config: &Config, 
        qh: &QueueHandle<WMState>
    ) {
        if let Some(entries) = config.mousebinds.clone() {
            for(key_string, action) in &entries {
                let (modmask, remaining_key) =  parse_modmask(key_string);
                let button = match remaining_key.as_str() {
                    "BTN_LEFT" => 0x110,
                    "BTN_RIGHT" => 0x111,
                    _ => 0x110,
                };
                let my_action = Action::action_from_name(action.action.clone(), &action.args);

                let proxy = self.proxy.get_pointer_binding(button, modmask, qh, self.proxy.id());
                proxy.enable();
                let binding = PointerBinding { proxy, action: my_action };
                self.pointer_bindings.insert(binding.proxy.id(), binding);
            }
        }
    }

    pub fn op_end(&mut self) {
        if let SeatOp::Resize { window_proxy, .. } = &self.op {
            window_proxy.inform_resize_end();
        }
        self.proxy.op_end();
        self.op = SeatOp::None;
    }

    pub fn op_manage(&mut self) {
        match &self.op {
            SeatOp::None | SeatOp::Move { .. } => { }
            SeatOp::Resize {
                window_proxy,
                start_w,
                start_h,
                edges, 
                ..
            } => {
                let (mut w, mut h) = (*start_w, *start_h);
                if edges.contains(Edges::Left) {
                    w -= self.op_dx;
                }
                if edges.contains(Edges::Right) {
                    w += self.op_dx;
                }
                if edges.contains(Edges::Top) {
                    h -= self.op_dy;
                }
                if edges.contains(Edges::Bottom) {
                    h += self.op_dy;
                }
                window_proxy.propose_dimensions(w.max(1), h.max(1));
            }
        }
    }

    pub fn pointer_move(&mut self, window: &Window) {
        self.interacted = Some(window.proxy.clone());
        self.proxy.op_start_pointer();
        self.op = SeatOp::Move {
            window_proxy: window.proxy.clone(),
            start_x: window.geom.x,
            start_y: window.geom.y,
        };
        self.op_dx = 0;
        self.op_dy = 0;
    }

    pub fn pointer_resize(&mut self, window: &Window, edges: Edges) {
        self.interacted = Some(window.proxy.clone());
        self.proxy.op_start_pointer();
        window.proxy.inform_resize_start();
        self.op = SeatOp::Resize {
            window_proxy: window.proxy.clone(),
            start_x: window.geom.x,
            start_y: window.geom.y,
            start_w: window.geom.w,
            start_h: window.geom.h,
            edges,
        };
        self.op_dx = 0;
        self.op_dy = 0;
    }
}

//--- Dispatches -----
impl Dispatch<RiverSeatV1, ()> for WMState {
    fn event(
        state: &mut Self,
        _proxy: &RiverSeatV1,
        event: <RiverSeatV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ){
        use crate::protocol::river_wm::river_seat_v1::Event;
        let seat = state.seat.as_mut().unwrap();
        match event {
            Event::Removed => { } 
            Event::WlSeat { name: _ } => { }
            Event::PointerEnter { window } => {
                if seat.ignore_pointer_enter_event {
                    seat.ignore_pointer_enter_event = false;
                } else {
                    println!("-----> [ PointerEnter event ]");
                    seat.hovered = Some(window);
                }
            }
            Event::PointerLeave => {
                    println!("-----> [ PointerLeave event ]");
                    seat.hovered = None;
            }
            Event::WindowInteraction { window } => seat.interacted = Some(window),
            Event::ShellSurfaceInteraction { shell_surface: _shell_surface } => { }
            Event::OpDelta { dx, dy } => (seat.op_dx, seat.op_dy) = (dx, dy),
            Event::OpRelease => seat.op_release = true,
            Event::PointerPosition { x, y } => { 
                (seat.cursor_x, seat.cursor_y) = (x, y);
                for(oid, output) in &mut state.outputs {
                    let geom = output.full_area;
                    if x >= geom.x && x < geom.x+geom.w && y >= geom.y && y < geom.y+geom.h && &state.focused_output_id != oid {
                        println!(" -> PointerPosition: focused output = {}", oid);
                        state.focused_output_id = oid.clone();
                        if let Some(window) = state.windows.get(&output.focused_window_id) {
                            seat.proxy.focus_window(&window.proxy);
                            state.needs_arrange=true;
                        }
                        if let Some(ls_output) = &output.ls_output {
                            ls_output.set_default();
                        }
                    }
                }
            }
        }
    }
}

impl Dispatch<RiverXkbBindingV1, ObjectId> for WMState {
    fn event(
        state: &mut Self,
        proxy: &RiverXkbBindingV1,
        event: <RiverXkbBindingV1 as Proxy>::Event,
        _data: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ){
        use crate::protocol::river_wm::river_xkb_binding_v1::Event;
        let seat = state.seat.as_mut().unwrap();
        let binding = seat.xkb_bindings.get(&proxy.id()).expect("xkb_binding not found");
        match event {
            Event::Pressed => seat.pending_action = binding.action.clone(),
            Event::Released => { }
            Event::StopRepeat => { }
        }
    }
}

impl Dispatch<RiverPointerBindingV1, ObjectId> for WMState {
    fn event(
        state: &mut Self,
        proxy: &RiverPointerBindingV1,
        event: <RiverPointerBindingV1 as Proxy>::Event,
        _data: &ObjectId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ){
        use crate::protocol::river_wm::river_pointer_binding_v1::Event;
        let seat = state.seat.as_mut().unwrap();
        let binding = seat.pointer_bindings.get(&proxy.id()).expect("pointer_binding not found");
        match event {
            Event::Pressed => seat.pending_action = binding.action.clone(),
            Event::Released => { }
        }
    }
}

impl Dispatch<RiverLayerShellSeatV1, ()> for WMState {
    fn event(
        _state: &mut Self,
        _proxy: &RiverLayerShellSeatV1,
        event: <RiverLayerShellSeatV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ){
        use crate::protocol::river_wm::river_layer_shell_seat_v1::Event;
        match event {
            Event::FocusExclusive => {
                println!("RLS-SeatV1::FocusExclusive");
                
            }
            Event::FocusNonExclusive => {
                println!("RLS-SeatV1::FocusNonExclusive");

            }
            Event::FocusNone => {
                println!("RLS-SeatV1::FocusNone");

            }
        }
    }
}

impl Dispatch<RiverInputManagerV1, ()> for WMState {
    fn event(
        _state: &mut Self,
        _proxy: &RiverInputManagerV1,
        _event: <RiverInputManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // input_manager events 
        //use crate::protocol::river_wm::river_input_manager_v1::Event;
        //match event {
        //    Event::InputDevice {id: _} => {
        //        println!("new input device");
        //    }
        //    Event::Finished => {
        //        println!("input_manager finished");
        //    }
        //}
    }

    wayland_client::event_created_child!(
        WMState,
        RiverInputManagerV1, 
        [ 1 => (RiverInputDeviceV1, ()) ]
    );
}

impl Dispatch<RiverInputDeviceV1, ()> for WMState {
    fn event(
        state: &mut Self,
        proxy: &RiverInputDeviceV1,
        event: <RiverInputDeviceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_wm::river_input_device_v1::Event;
        let device_info = state.riverinput_devices.entry(proxy.clone()).or_insert(DeviceInfo {
            name: String::new(),
            device_type: None,
            libinput_device: None,
        });

        match event {
            Event::Name { name } => {
                device_info.name = name;
            }
            Event::Type { _type } => {
                device_info.device_type = _type.into_result().ok();
            }
            _ => { }
        }
    }
}

impl Dispatch<RiverLibinputConfigV1, ()> for WMState {
    fn event(
        _state: &mut Self,
        _proxy: &RiverLibinputConfigV1,
        _event: <RiverLibinputConfigV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // libinput_config events
    }

    wayland_client::event_created_child!(
        WMState,
        RiverLibinputConfigV1, 
        [ 1 => (RiverLibinputDeviceV1, ()) ]
    );
}

impl Dispatch<RiverLibinputDeviceV1, ()> for WMState {
    fn event(
        state: &mut Self,
        proxy: &RiverLibinputDeviceV1,
        event: <RiverLibinputDeviceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_wm::river_libinput_device_v1::Event;
        match event {
            Event::InputDevice{ device } => {
                let device_info = state.riverinput_devices.get_mut(&device).expect("no device_info found");
                device_info.libinput_device = Some(proxy.clone());
                //println!("HERE Device name = {}", device_info.name);
            }
            Event::TapSupport { finger_count } => {
                if finger_count > 0 {
                    if let Some(device_info) = state.riverinput_devices.values().find(|d| d.libinput_device==Some(proxy.clone())) {
                        println!("Tap-to-click is supported on device {}", device_info.name);

                        let enable_tap = if state.config.inputs.touchpad_tap_click {
                            crate::protocol::river_wm::river_libinput_device_v1::TapState::Enabled
                        } else {
                            crate::protocol::river_wm::river_libinput_device_v1::TapState::Disabled
                        };

                        proxy.set_tap(enable_tap, qh, ());
                    }
                }
            }
            _ => { }
        }
    }

    wayland_client::event_created_child!(
        WMState,
        RiverLibinputDeviceV1, 
        [ _ => (RiverLibinputResultV1, ()) ]
    );
}

impl Dispatch<RiverLibinputResultV1, ()> for WMState {
    fn event(
        _state: &mut Self,
        _proxy: &RiverLibinputResultV1,
        _event: <RiverLibinputResultV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // libinput_result events
    }
}

impl Dispatch<RiverXkbConfigV1, ()> for WMState {
    fn event(
        state: &mut Self,
        _proxy: &RiverXkbConfigV1,
        event: <RiverXkbConfigV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_wm::river_xkb_config_v1::Event;
        
        match event {
            Event::XkbKeyboard { id } => {
                state.keyboards.push(id.clone());

                let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
                let rules = "evdev".to_string();
                let model = "pc105".to_string();
                let layout = state.config.inputs.xkb_layout.clone();
                let variant = "".to_string();
                let options = state.config.inputs.xkb_options.clone();

                let keymap = xkb::Keymap::new_from_names(
                    &context,
                    &rules,
                    &model,
                    &layout,
                    &variant,
                    Some(options),
                    xkb::KEYMAP_COMPILE_NO_FLAGS,
                );

                if let Some(map) = keymap {
                    let keymap_str = map.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
                    let mut temp_file = tempfile().expect("");
                    let _ = temp_file.write_all(keymap_str.as_bytes());

                    if let Some(manager) = &state.xkb_config {
                        let _river_keymap = manager.create_keymap(temp_file.as_fd(), KeymapFormat::TextV1, qh, ());
                    }
                }
            }
            _ => { }
        }
    }
    wayland_client::event_created_child!(
        WMState,
        RiverXkbConfigV1,
        [ 1 => (RiverXkbKeyboardV1, ()) ]
    );
}

impl Dispatch<RiverXkbKeyboardV1, ()> for WMState {
    fn event(
        _state: &mut Self,
        _proxy: &RiverXkbKeyboardV1,
        _event: <RiverXkbKeyboardV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<RiverXkbKeymapV1, ()> for WMState {
    fn event(
        state: &mut Self,
        proxy: &RiverXkbKeymapV1,
        event: <RiverXkbKeymapV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_wm::river_xkb_keymap_v1::Event;
        match event {
            Event::Success => {
                //println!("Success: setting keymap");
                for kb in &state.keyboards {
                    kb.set_keymap(proxy);
                }
            }
            Event::Failure {error_msg } => {
                println!("Could not set keymap: {error_msg}");
            }
            //_ => { }
        }
    }
}

