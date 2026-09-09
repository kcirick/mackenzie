use crate::protocol::river_wm::river_output_v1::RiverOutputV1;
use crate::protocol::river_wm::river_layer_shell_output_v1::RiverLayerShellOutputV1;

//use wayland_backend::client::ObjectId;
use wayland_client::{protocol::wl_output, Connection, Dispatch, Proxy, QueueHandle};

use crate::wmcore::WMState;
use crate::layout::Geometry;

//--- Struct -----
#[derive(Debug)]
pub struct Output {
    pub proxy: RiverOutputV1,
    pub name: String,
    pub removed: bool,
    pub full_area: Geometry,
    pub usable_area: Geometry,
    
    pub visible_tags: u16,
    //pub visible_columns_id: Vec<i32>,
    pub focused_column_id: i32,

    pub ls_output: Option<RiverLayerShellOutputV1>,
}

#[derive(Debug, Clone)]
pub struct WlOutputInfo {
    pub id: u32,
    pub proxy: wl_output::WlOutput,
    pub name: String,
}

//--- Implementation -----
impl Output {
    pub fn new(proxy: RiverOutputV1, current_tag: u16) -> Self {
        Self {
            proxy,
            name: String::new(),
            removed: false,
            full_area: Geometry { x:0, y:0, w:0, h:0 },
            usable_area: Geometry { x:0, y:0, w:0, h:0 },

            visible_tags: current_tag,
            //visible_columns_id: Vec::new(),
            focused_column_id: 0,

            ls_output:None,
        }
    }
}

//--- Dispatches -----
impl Dispatch<RiverOutputV1, ()> for WMState {
    fn event(
        state: &mut Self,
        proxy: &RiverOutputV1,
        event: <RiverOutputV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ){
        use crate::protocol::river_wm::river_output_v1::Event;
        let output = state.outputs.get_mut(&proxy.id()).expect("Output not found");
        match event {
            Event::Removed => output.removed = true,
            Event::WlOutput { name: id } => { 
                if let Some(wloutput) = state.wl_output_info.iter().find(|o| o.id == id) {
                    //println!("Match Wloutput with name {}", wloutput.name);
                    output.name = wloutput.name.clone();
                }
            }
            Event::Position { x, y } => { 
                println!("Output {:?} Position: +{}+{}", proxy.id(), x, y);
                output.full_area.x = x;
                output.full_area.y = y;
            }
            Event::Dimensions { width, height } => { 
                println!("Output {:?} Resolution: {}x{}", proxy.id(), width, height);
                output.full_area.w = width;
                output.full_area.h = height;
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for WMState {
    fn event(
        state:&mut Self,
        proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ){
        match event {
            wl_output::Event::Name { name } => {
                if let Some(wloutput) = state.wl_output_info.iter_mut().find(|o| &o.proxy == proxy) {
                    wloutput.name = name;
                    println!(" Output id: {} / name: {} - not registered", wloutput.id, wloutput.name);
                }
            }
            wl_output::Event::Description { description } => {
                println!(" Output description: {description}");
            }
            _ => { }
        }
    }
}

