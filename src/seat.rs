use std::collections::HashMap;
use xkbcommon::xkb;

use crate::protocol::river_wm::{
    river_window_v1::{Edges,RiverWindowV1},
    river_seat_v1::{Modifiers, RiverSeatV1},
    river_pointer_binding_v1::RiverPointerBindingV1,
    river_xkb_binding_v1::RiverXkbBindingV1,
    river_xkb_bindings_v1::RiverXkbBindingsV1,
};

use crate::actions::Action;
use crate::config::Config;
use crate::wmcore::WMState;
use crate::wmcore::Window;

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
    //println!("Remaining key = {remaining_key}");
    return (modmask, remaining_key)
} 

//--- Structs -----
#[derive(Debug)]
pub struct Seat {
    pub proxy: RiverSeatV1,
    pub new: bool,
    //pub removed: bool,
    //pub focused: Option<RiverWindowV1>,
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
            new: true, 
            //removed: false,
            //focused: None,
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
                    if x >= geom.x && x < geom.x+geom.w && y >= geom.y && y < geom.y+geom.h {
                        if &state.focused_output_id != oid {
                            println!("focused output = {}", oid);
                            state.focused_output_id = oid.clone();
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

