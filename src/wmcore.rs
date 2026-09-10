use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{Read,Write};
use std::os::unix::net::{UnixListener,UnixStream};

use serde::{Serialize};
use serde_json;

use wayland_backend::client::ObjectId;
use wayland_client::{protocol::{wl_output, wl_registry}, Connection, Dispatch, Proxy, QueueHandle};

use crate::actions::Action;
use crate::config::Config;
use crate::output::Output;
use crate::output::WlOutputInfo;
use crate::seat::Seat;
use crate::seat::SeatOp;
use crate::layout::Geometry;
use crate::layout::Column;
use crate::layout::create_new_column;
use crate::layout::arrange;

//--- Protocols -----
use crate::protocol::river_wm::{
    river_node_v1::RiverNodeV1,
    river_output_v1::RiverOutputV1,
    river_seat_v1::RiverSeatV1,
    river_window_manager_v1::RiverWindowManagerV1,
    river_window_v1::{Edges, RiverWindowV1},
    river_xkb_bindings_v1::RiverXkbBindingsV1,
    river_layer_shell_v1::RiverLayerShellV1,
    river_layer_shell_output_v1::RiverLayerShellOutputV1,
};

//--- Enums -----
#[derive(Debug, PartialEq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}


//--- Helper functions -----
pub fn parse_hex_color(hex: &str) -> (u32, u32, u32, u32) {
    let hex = hex.trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        6 => {
            let r = u32::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u32::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u32::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            (r, g, b, 255)
        }
        8 => {
            let r = u32::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u32::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u32::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            let a = u32::from_str_radix(&hex[8..8], 16).unwrap_or(255);
            (r, g, b, a)
        } 
        _ => (0, 0, 0, 255)
    };

    // Scale 8-bit to 32-bit range and pre-multiply alpha
    let a32 = (a as u32) * 0x01010101;
    let scale = (a as u32) / 255;
    let r32 = (r as u32 * scale) * 0x01010101;
    let g32 = (g as u32 * scale) * 0x01010101;
    let b32 = (b as u32 * scale) * 0x01010101;
    
    (r32, g32, b32, a32)
}


//--- Structs -----
#[derive(Debug)]
pub struct WMState {
    pub river_wm: Option<RiverWindowManagerV1>,
    pub river_xkb: Option<RiverXkbBindingsV1>,
    pub layer_shell_manager: Option<RiverLayerShellV1>,

    pub config: Config,
    
    // Master list of seat, outputs, workspaces, columns and windows
    pub seat: Option<Seat>,
    pub wl_output_info: Vec<WlOutputInfo>,
    pub outputs: HashMap<ObjectId, Output>,
    pub columns: Vec<Column>,
    pub windows: HashMap<ObjectId, Window>,

    pub focused_output_id: ObjectId,
    pub focused_window_id: ObjectId,
    pub focused_tag: u16,

    pub needs_arrange: bool,

    // IPC stuff
    pub ipc_update_requested: bool,
    pub ipc_listener: Option<UnixListener>, 
    pub ipc_clients: Vec<UnixStream>,
}

#[derive(Debug, Clone)]
pub struct Window {
    pub proxy: RiverWindowV1,
    pub node: RiverNodeV1,
    pub column_id: i16,

    new: bool,
    closed: bool,
    
    pub geom: Geometry,
    pub float_geom: Geometry,

    pub resize_requested: bool,
    pub at_scroll_edge: bool,
    pub is_floating: bool,

    pointer_move_requested: Option<RiverSeatV1>,
    pointer_resize_requested: Option<RiverSeatV1>,
    pointer_resize_requested_edges: Edges,
}

#[derive(Serialize)]
struct WorkspaceInfo {
    output: String,
    tags: Vec<TagInfo>,
}

#[derive(Serialize)]
struct TagInfo {
    index: u16,
    is_active: bool,
    is_occupied: bool,
}

//--- Implementations -----
impl WMState {
    pub fn new() -> Self {
        WMState {
            river_wm: None,
            river_xkb: None,
            layer_shell_manager: None,

            config: Config::default(), 
            
            seat: None,
            wl_output_info: Vec::new(),
            outputs: HashMap::new(),
            columns: Vec::new(),
            windows: HashMap::new(),

            focused_output_id: ObjectId::null(),
            focused_window_id: ObjectId::null(),
            focused_tag: (1 << 0),

            needs_arrange: false,

            ipc_update_requested: false,
            ipc_listener: None, 
            ipc_clients: Vec::new(),
        }
    }

    fn handle_manage_start(
        &mut self,
        proxy: &RiverWindowManagerV1,
        _qh: &QueueHandle<Self>,
    ){
        println!("\n[ handle_manage_start ]");

        let seat = self.seat.as_mut().unwrap();
        if seat.pending_action != Action::None {
            self.ipc_update_requested = true;
        }
        Action::do_action(self, &proxy);

        self.remove_unneeded();
        self.init_new();
        self.manage();

        proxy.manage_finish();
    }

    fn handle_render_start(
        &mut self,
        proxy: &RiverWindowManagerV1,
    ) {
        //println!("-----> [ handle_render_start ]");
        let seat = self.seat.as_mut().unwrap();
        match &seat.op {
            SeatOp::None => {}
            SeatOp::Move { window_proxy, start_x, start_y } => {
                if let Some(window) = self.windows.get_mut(&window_proxy.id()) {
                        window.set_position(start_x + seat.op_dx, start_y + seat.op_dy);
                }
            }
            SeatOp::Resize { window_proxy, start_x, start_y, start_w, start_h, edges } => {
                if let Some(window) = self.windows.get_mut(&window_proxy.id()) {
                        let (mut x, mut y) = (*start_x, *start_y);
                        if edges.contains(Edges::Left) {
                            x += start_w - window.geom.w;
                        }
                        if edges.contains(Edges::Top) {
                            y += start_h - window.geom.h;
                        }
                        window.set_position(x, y);
                    }
            }
        }

        // Arrange should be here
        if self.needs_arrange {
            arrange(self);
            self.needs_arrange = false;
        }

        proxy.render_finish();
    }

    fn remove_unneeded(
        &mut self
    ) {
        //--- Remove old outputs
        self.outputs.retain(|_, output| {
            if output.removed {
                output.proxy.destroy();
                return false;
            }
            true
        });

        //--- Remove old windows
        let seat = self.seat.as_mut().unwrap();
        for (_, window) in self.windows.iter().filter(|(_,w)| w.closed) {
            if let SeatOp::Move {window_proxy, .. } | 
                SeatOp::Resize { window_proxy, .. } = &seat.op {
                    if window_proxy == &window.proxy {
                        seat.op_end();
                    }
                }

            let (index, column) = self.columns.iter_mut().enumerate().find(|(_,c)| c.id == window.column_id)
                .expect("column not found");
            let nwins = self.windows.iter().filter(|(_, w)| w.column_id==column.id).count();
            let column_id = column.id;
            let column_index = index; 
            if nwins==1 {
                // this is the only window in the column, so safe to delete
                self.columns.retain(|c| c.id != column_id);
                println!("column size = {} / column index = {}", self.columns.len(), column_index);
                if window.proxy.id() == self.focused_window_id {
                    if self.columns.len()>0 {
                        let prev_column = self.columns.get(if column_index==0 {0} else {column_index-1}).unwrap();
                        //let first_win_id_in_prev_column = prev_column.windows_id.get(0).unwrap();
                        let (win_id, _win) = self.windows.iter()
                            .find(|(_, w)| w.column_id==prev_column.id).unwrap();
                        self.focused_window_id = win_id.clone();
                    } else {
                        self.focused_window_id = ObjectId::null(); 
                    }
                }
            } else {
                println!("remove nwins >1");
                let old_win_index = column.windows_id.iter()
                    .position(|wid| wid == &window.proxy.id()).unwrap();
                column.windows_id.retain(|wid| wid != &window.proxy.id());

                let new_win_index = if old_win_index==0 {0} else { old_win_index-1 };

                self.focused_window_id = column.windows_id[new_win_index].clone();
                // Flag for redistribution
                column.redistribute_requested = true;
            }
            println!("needs_arrange from remove_old windows");
            self.needs_arrange = true;
        }
        self.windows.retain(|_, w| !w.closed);

        //--- Remove empty columns
        self.columns.retain(|c| {
            if self.windows.iter().filter(|(_,w)| w.column_id == c.id).count() > 0 {
                true
            } else { false }
        });
    }

    fn init_new(
        &mut self, 
    ) {
        //--- Init new windows
        let mut last_column_index = 0;
        let mut max_column_id = 0;
        if let Some(window) = self.windows.get(&self.focused_window_id) {
            last_column_index = self.columns.iter()
                .position(|c| c.id == window.column_id).unwrap();
            max_column_id = self.windows.iter()
                .max_by_key(|(_, w)| w.column_id)
                .map(|(_, w)| w.column_id).unwrap();
        }

        let mut new_column_id = max_column_id + 1;
        let focused_output = self.outputs.get_mut(&self.focused_output_id)
            .expect("No focused outputs");
        let output_area = focused_output.usable_area;
        let ncols_focused_output = self.columns.iter().filter(|c| c.output_id==self.focused_output_id).count();

        let gap = self.config.layout.gap + self.config.window.border_width;
        let edge_gap = self.config.layout.scroll_edge_gap;
        let column_width_ratio = self.config.layout.default_column_width;
        //let seat = self.seat.as_mut().unwrap();
        for (_,window) in self.windows.iter_mut().filter(|(_, w)| w.new) {

            // Set the new dimension (but not position)
            //window.geom.x = output_area.x + gap;
            //window.geom.y = output_area.y + gap;
            window.geom.w = (((output_area.w - 2*edge_gap - 2*gap) as f32) * column_width_ratio) as i32;
            window.geom.h = output_area.h - 2*gap;
            
            // Create a new column
            create_new_column(
                &mut self.columns,
                new_column_id,
                last_column_index+1,
                window,
                focused_output,
                self.focused_tag,
            );

            // If there are no focused window, make this the focused window
            if self.focused_window_id == ObjectId::null() || ncols_focused_output==0 {
                self.focused_window_id = window.proxy.id();
                //seat.proxy.focus_window(&window.proxy);
                focused_output.focused_column_id = new_column_id;
            }

            window.proxy.use_ssd();
            window.new = false;
            
            new_column_id += 1;
            last_column_index += 1;

            println!(" |-> new window geometry = {}x{}+{}+{}", window.geom.w, window.geom.h, window.geom.x, window.geom.y);
            //window.set_position(window.geom.x, window.geom.y);
            window.proxy.propose_dimensions(window.geom.w, window.geom.h);

            println!("needs_arrange from init_new windows");
            self.needs_arrange = true;
        }
    }

    fn manage(
        &mut self, 
    ) {
        let seat = self.seat.as_mut().unwrap();

        for window in self.windows.values_mut() {
            if let Some(_) = window.pointer_move_requested.take() {
                seat.pointer_move(window);
            }
            if let Some(_) = window.pointer_resize_requested.take() {
                seat.pointer_resize(window, window.pointer_resize_requested_edges);
            }
        }

        for column in self.columns.iter_mut().filter(|c| c.redistribute_requested) {
            let focused_output = self.outputs.get(&self.focused_output_id).unwrap(); 
            let output_area = focused_output.usable_area;
            let gap = self.config.layout.gap + self.config.window.border_width;
            let target_height = output_area.h-2*gap;

            let nwins = column.windows_id.len();
            let mut col_height = 0;
            for i in 0..nwins {
                let window = self.windows.get_mut(&column.windows_id[i]).unwrap();
                if i<nwins-1 {
                    col_height += window.geom.h + gap;
                } else {
                    window.geom.h = target_height - col_height;
                    window.resize_requested = true;
                }
            }
            column.redistribute_requested = false;
        }

        let seat = self.seat.as_mut().unwrap();
        for (_,window) in &mut self.windows {
            // Sloppy focus on hovered windows (unless they are at the edge or sloppy focus is
            // disabled)
            if let Some(hovered_window_proxy) = seat.hovered.as_ref() {
                if &window.proxy==hovered_window_proxy && !window.at_scroll_edge { 
                    println!(" |--> focusing on a hovered window"); 
                    if hovered_window_proxy.id() != self.focused_window_id {
                        println!("needs_arrange from sloppy focus");
                        self.needs_arrange = true;
                    }
                    seat.proxy.focus_window(&window.proxy);
                    self.focused_window_id = window.proxy.id();
                    let output = self.outputs.get_mut(&self.focused_output_id).unwrap();
                    output.focused_column_id = window.column_id;
                }
            }
            // Click to raise
            if let Some(interacted_window_proxy) = seat.interacted.as_ref() {
                if &window.proxy==interacted_window_proxy {
                    if window.at_scroll_edge {
                        seat.proxy.focus_window(&window.proxy);
                        self.focused_window_id = window.proxy.id();
                        let output = self.outputs.get_mut(&self.focused_output_id).unwrap();
                        output.focused_column_id = window.column_id;
                        self.needs_arrange = true;
                    }
                    window.node.place_top();
                }
            } 

            if window.resize_requested {
                println!("handling resize_requeted");
                window.proxy.propose_dimensions(window.geom.w, window.geom.h);
                window.resize_requested = false;
            }
        }

        if seat.op_release {
            seat.op_end();
            seat.op_release = false;
        } else {
            seat.op_manage();
        }

        let focused_output = self.outputs.get(&self.focused_output_id).unwrap();
        if let Some(ls_output) = &focused_output.ls_output {
            ls_output.set_default();
        }
    }

    pub fn handle_ipc_connections (
        &mut self,
    ) {
        let listener = self.ipc_listener.as_ref().expect("No listener active");
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.set_nonblocking(true);

            let mut buffer = [0; 1024];
            // ipc_type Set command
            if let Ok(n) = stream.read(&mut buffer) && n>0 {
                let command = String::from_utf8_lossy(&buffer[..n]).trim().to_string();
                let mut v_cmd: Vec<String> = command.split_whitespace().map(String::from).collect();
                let v_arg = Some(v_cmd.split_off(1));

                let seat = self.seat.as_mut().unwrap();
                seat.pending_action = Action::action_from_name(v_cmd[0].to_string(), &v_arg); 

                self.river_wm.as_ref().expect("river_wm expected").manage_dirty();
                let _ = stream.write_all("success".as_bytes());
                return;
            }

            let initial_response = self.get_tags_status_json();
            let _ = stream.write_all(initial_response.as_bytes());

            self.ipc_clients.push(stream);
        }
    }

    pub fn ipc_broadcast (
        &mut self,
    ) {
        if self.ipc_clients.is_empty() { return; }

        let json_workspace = self.get_tags_status_json(); 
        println!("Workspace Info: \n{}", json_workspace);

        self.ipc_clients.retain_mut(|c| Write::write_all(c, json_workspace.as_bytes()).is_ok());
    }
    
    fn get_tags_status_json(&self) -> String {
        let n_tags = self.config.layout.n_tags;
        let mut ws_result: Vec<WorkspaceInfo> = Vec::new();
        for (_,output) in self.outputs.iter() {
            let mut output_info = WorkspaceInfo {
                output: output.name.clone(),
                tags: Vec::new(),
            };
            for i in 1..(n_tags+1) {
                let n_columns = self.columns.iter()
                    .filter(|c| c.output_id==output.proxy.id() && c.tag & (1 << (i-1)) >0)
                    .count();
                let tag_info = TagInfo {
                    index: i,
                    is_active: (1 << (i-1)) & output.visible_tags > 0,
                    is_occupied: n_columns>0,
                };
                output_info.tags.push(tag_info);
            }
            ws_result.push(output_info);
        }

        serde_json::to_string(&ws_result).unwrap_or_default()
    }
}

impl Window {
    fn new(
        proxy: RiverWindowV1,
        qh: &QueueHandle<WMState>,
    ) -> Self {
        let node = proxy.get_node(qh, ());
        Window {
            proxy,
            node,
            column_id: 0,

            new: true,
            closed: false,
            
            geom: Geometry { x:0, y:0, w:0, h:0 },
            float_geom: Geometry { x:0, y:0, w:0, h:0 },

            resize_requested: false,
            at_scroll_edge: false,
            is_floating: false,

            pointer_move_requested: None,
            pointer_resize_requested: None,
            pointer_resize_requested_edges: Edges::None,
        }
    }

    fn set_position(&mut self, x: i32, y: i32) {
        self.node.set_position(x, y);
        self.geom.x = x;
        self.geom.y = y;
    }
}


//--- Dispatches -----
impl Dispatch<wl_registry::WlRegistry, ()> for WMState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = 
        event {
            const RIVER_WINDOW_MANAGER_V1_VERSION: u32 = 4;
            const RIVER_XKB_BINDINGS_V1_VERSION: u32 = 2;
            const RIVER_LAYER_SHELL_V1_VERSION: u32 = 1;
            const WL_OUTPUT_VERSION: u32 = 4;
            //println!("==> REGISTRY INTERFACE = {}", interface);
            match interface.as_str() {
                "river_window_manager_v1" => {
                    if version < RIVER_WINDOW_MANAGER_V1_VERSION {
                        eprintln!("Server river_window_manager_v1 = v{version}");
                        std::process::exit(1);
                    }
                    let wm = registry.bind::<RiverWindowManagerV1, _, _> (
                        name, 
                        RIVER_WINDOW_MANAGER_V1_VERSION, 
                        qh, () );
                    state.river_wm = Some(wm);
                }
                "river_xkb_bindings_v1" => {
                    let xkb = registry.bind::<RiverXkbBindingsV1, _, _> (
                        name, 
                        RIVER_XKB_BINDINGS_V1_VERSION, 
                        qh, () );
                    state.river_xkb = Some(xkb);
                }
                "river_layer_shell_v1" => {
                    let rls = registry.bind::<RiverLayerShellV1, _, _> (
                        name, 
                        RIVER_LAYER_SHELL_V1_VERSION, 
                        qh, () );
                    state.layer_shell_manager = Some(rls);
                }
                "wl_output" => {
                    let output = registry.bind::<wl_output::WlOutput, _, _>(
                        name,
                        WL_OUTPUT_VERSION,
                        qh, () );
                    state.wl_output_info.push(WlOutputInfo{id: name, proxy: output, name:"".to_string()});
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<RiverWindowManagerV1, ()> for WMState {
    fn event(
        state: &mut Self,
        proxy: &RiverWindowManagerV1,
        event:<RiverWindowManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_wm::river_window_manager_v1::Event;
        match event {
            Event::Unavailable => {
                eprintln!("Error: Another WM is already running");
                std::process::exit(1);
            }
            Event::Finished => std::process::exit(0),
            Event::ManageStart => {
                state.handle_manage_start(proxy, qh)
            }
            Event::RenderStart => {
                state.handle_render_start(proxy);
            }
            Event::SessionLocked => {}
            Event::SessionUnlocked => {}
            Event::Window { id } => {
                state.windows.insert(id.id(), Window::new(id.clone(), qh));
            }
            Event::Output { id } => { 
                state.outputs.insert(id.id(), Output::new(id.clone(), state.focused_tag));
                let new_output = state.outputs.get_mut(&id.id()).unwrap();
                if let Some(ls_manager) = &state.layer_shell_manager {
                    let ls_output = ls_manager.get_output(&id, qh, ());
                    new_output.ls_output = Some(ls_output);
                    println!("Registered layer-shell output");
                }
            }
            Event::Seat { id } => { 
                println!("New Seat");
                let mut this_seat = Seat::new(id);
                let this_river_xkb = state.river_xkb.as_ref()
                    .expect("river_xkb_bindings_v1_missing");
                this_seat.keybinds_from_config(this_river_xkb, &state.config, qh);
                this_seat.mousebinds_from_config(&state.config, qh);
                //seat.new = false;
                state.seat = Some(this_seat);
            }
        }
    }

    wayland_client::event_created_child!(WMState, RiverWindowManagerV1, [
        crate::protocol::river_wm::river_window_manager_v1::EVT_WINDOW_OPCODE => (RiverWindowV1, ()),
        crate::protocol::river_wm::river_window_manager_v1::EVT_OUTPUT_OPCODE => (RiverOutputV1, ()),
        crate::protocol::river_wm::river_window_manager_v1::EVT_SEAT_OPCODE => (RiverSeatV1, ())
    ]);
}

impl Dispatch<RiverWindowV1, ()> for WMState {
    fn event(
        state: &mut Self,
        proxy: &RiverWindowV1,
        event: <RiverWindowV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ){
        use crate::protocol::river_wm::river_window_v1::Event;
        let (_, window) = match state.windows.iter_mut().find(|(_, w)| &w.proxy == proxy) {
            Some(window) => window,
            None => return,
        };
        match event {
            Event::Closed => window.closed = true,
            Event::DimensionsHint {
                min_width: _,
                min_height: _,
                max_width: _,
                max_height: _,
            } => { }
            Event::Dimensions { width, height } => {
                if window.geom.w != width || window.geom.h != height {
                    //println!("Event::Dimensions");
                    //println!("window {}x{}, event geom {}x{}", window.geom.w, window.geom.h, width, height);
                    (window.geom.w, window.geom.h) = (width, height);
                    window.resize_requested = true;
                }
            }
            Event::AppId { app_id: _ } => { }
            Event::Title { title: _ } => { }
            Event::Parent { parent: _ } => { }
            Event::DecorationHint { hint: _ } => { }
            Event::PointerMoveRequested { seat } => window.pointer_move_requested = Some(seat),
            Event::PointerResizeRequested { seat, edges } => {
                window.pointer_resize_requested = Some(seat);
                window.pointer_resize_requested_edges = edges.into_result().expect("Invalid edges for resize");
            }
            Event::ShowWindowMenuRequested { x: _, y: _ } => { }
            Event::MaximizeRequested => { }
            Event::UnmaximizeRequested => { }
            Event::FullscreenRequested { output: _ } => { }
            Event::ExitFullscreenRequested => { }
            Event::MinimizeRequested => { }
            Event::UnreliablePid { unreliable_pid: _ } => { }
            Event::PresentationHint { .. } => { }
            Event::Identifier { .. } => { }
        }
    }
}

impl Dispatch<RiverLayerShellOutputV1, ()> for WMState {
    fn event(
        state: &mut Self,
        _proxy: &RiverLayerShellOutputV1,
        event: <RiverLayerShellOutputV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::river_wm::river_layer_shell_output_v1::Event;
        match event {
            Event::NonExclusiveArea { x, y, width, height } => {
                //setting non-exlusive area
                let center_x = x + (width / 2);
                let center_y = y + (height / 2);

                for (id, this_output) in &mut state.outputs {
                    let this_geom = this_output.full_area;

                    if this_geom.w >0 && center_x >= this_geom.x 
                        && center_x < this_geom.x + this_geom.w
                        && center_y >= this_geom.y
                        && center_y < this_geom.y + this_geom.h
                    {
                        println!("Reservation request for output {id}: {width}x{height}+{x}+{y}");
                        this_output.usable_area = Geometry {x, y, w: width, h: height};
                        //this_output.ls_output = Some(proxy.clone());
                    }
                }
            }
        }
    }
}

wayland_client::delegate_noop!(WMState: ignore RiverXkbBindingsV1);
wayland_client::delegate_noop!(WMState: ignore RiverNodeV1);
wayland_client::delegate_noop!(WMState: ignore RiverLayerShellV1);
