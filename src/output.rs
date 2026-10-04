use crate::protocol::river_wm::river_output_v1::RiverOutputV1;
use crate::protocol::river_wm::river_layer_shell_output_v1::RiverLayerShellOutputV1;

use wayland_backend::client::ObjectId;
use wayland_client::{protocol::wl_output, Connection, Dispatch, Proxy, QueueHandle};

use crate::wmcore::WMState;
use crate::layout::Geometry;

//--- Struct -----
#[derive(Debug)]
pub struct Output {
    pub proxy: RiverOutputV1,
    pub wl_output_id: u32,
    pub name: String,
    pub removed: bool,
    pub full_area: Geometry,
    pub usable_area: Geometry,
    
    pub focused_tag: u16,
    pub visible_tags: u16,
    pub focused_window_id: ObjectId,

    pub ls_output: Option<RiverLayerShellOutputV1>,
}

#[derive(Debug, Clone)]
pub struct WlOutputInfo {
    pub proxy: wl_output::WlOutput,
    pub name: String,
    pub description: String,
}

//--- Implementation -----
impl Output {
    pub fn new(proxy: RiverOutputV1) -> Self {
        Self {
            proxy,
            wl_output_id: 0,
            name: String::new(),
            removed: false,
            full_area: Geometry { x:0, y:0, w:0, h:0 },
            usable_area: Geometry { x:0, y:0, w:0, h:0 },

            focused_tag: (1 << 0),
            visible_tags: (1 << 0),
            focused_window_id: ObjectId::null(),

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
                if let Some(wloutput) = state.wl_output_info.get(&id) {
                    output.wl_output_id = id;
                    output.name = wloutput.name.clone();
                }
            }
            Event::Position { x, y } => { 
                log::info!("Output {} Position: +{}+{}", proxy.id(), x, y);
                output.full_area.x = x;
                output.full_area.y = y;
            }
            Event::Dimensions { width, height } => { 
                log::info!("Output {} Resolution: {}x{}", proxy.id(), width, height);
                output.full_area.w = width;
                output.full_area.h = height;
            }
            _ => { }
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
                if let Some((id, wloutput)) = state.wl_output_info.iter_mut().find(|(_,o)| &o.proxy == proxy) {
                    log::info!("Output id: {id} / name: {name}");
                    wloutput.name = name.clone();
                    if let Some(output) = state.outputs.values_mut().find(|o| &o.wl_output_id == id) {
                        output.name = name.clone();
                    }
                }
            }
            wl_output::Event::Description { description } => {
                if let Some((id, wloutput)) = state.wl_output_info.iter_mut().find(|(_,o)| &o.proxy == proxy) {
                    log::info!("Output id: {id} / description: {description}");
                    wloutput.description = description.clone();
                }
            }
            _ => { }
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
                        log::info!("Reservation request for output {id}: {width}x{height}+{x}+{y}");
                        this_output.usable_area = Geometry {x, y, w: width, h: height};
                        //this_output.ls_output = Some(proxy.clone());
                        for column in state.columns.iter_mut().filter(|c| &c.output_id==id) {
                            column.redistribute_requested = true;
                        }
                        state.needs_arrange_ids.push(id.clone());
                    }
                }
            }
        }
    }
}

