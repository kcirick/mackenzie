
use wayland_backend::client::ObjectId;
use crate::protocol::river_wm::river_window_v1::Edges;
use wayland_client::Proxy;

use std::collections::HashMap;

use crate::wmcore::parse_hex_color;
use crate::wmcore::WMState;
use crate::wmcore::Window;
use crate::wmcore::Direction;
use crate::config::Config;
use crate::output::Output;

//--- Structs -----
#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub id: i16,
    pub output_id: ObjectId,
    pub windows_id: Vec<ObjectId>,

    pub geom: Geometry,
    pub prev_width: i32,

    pub tag: u16,
    pub is_maximized: bool,
    pub redistribute_requested: bool,
    pub align_width_requested: bool,
    pub center_requested: bool,
}

//--- Functions -----
pub fn create_new_column (
    output: &Output,
    columns_list: &Vec<Column>,
    window: &mut Window,
    config: &Config,
) -> Column {
    let bw = config.window.border_width;
    let gap = config.layout.gap;

    let mut max_column_id = 0;
    if columns_list.len() > 0 {
        max_column_id = columns_list.iter()
            .max_by_key(|c| c.id)
            .map(|c| c.id).unwrap();
    }
    let new_column_id = max_column_id+1;

    let mut column = Column { 
        id: new_column_id, 
        output_id: output.proxy.id().clone(),
        windows_id: Vec::new(),

        geom: window.geom,
        prev_width: window.geom.w,
        
        tag: output.focused_tag,
        is_maximized: false,
        redistribute_requested: false,
        align_width_requested: false,
        center_requested: false,
    };

    column.geom.h = output.usable_area.h - 2*gap - 2*bw;
    window.column_id = new_column_id;
    column.windows_id.push(window.proxy.id().clone());

    //println!(" |--> Created a new column with id = {} at position {}", new_column_id, new_column_index);
    column
}

fn render_focused_column(
    column: &mut Column,
    windows: &mut HashMap<ObjectId, Window>,
    focused_window: &Window,
    x_offset: i32,
    output_area: Geometry,
    config: &Config,
    is_focused_output: bool,
) {

    let bw = config.window.border_width;
    let gap = config.layout.gap;
    let edge_gap = if column.is_maximized {0} else {config.layout.scroll_edge_gap}; 

    let mut y_offset = output_area.y + gap + bw;
    let mut first_win = true;
    let mut x_correction = 0;

    for window_id in &column.windows_id { 
        let window = windows.get_mut(&window_id).unwrap();

        if window.is_floating { 
            let mut this_border_color = &config.window.border_color_unfocused;
            if window.proxy.id()==focused_window.proxy.id() {
                this_border_color = &config.window.border_color_focused;
            }
            let bcol = parse_hex_color(this_border_color.as_str());
            window.proxy.set_borders(Edges::all(), bw, bcol.0, bcol.1, bcol.2, bcol.3);

            window.node.place_top();

            continue; 
        }

        window.at_scroll_edge = false;

        window.geom.x = x_offset;
        window.geom.y = y_offset;
        y_offset += window.geom.h + gap + 2*bw;

        // Reset the clip box for focused column
        window.proxy.show();
        window.proxy.set_clip_box(0, 0, 0, 0);

        if first_win {
            // If the geometry is outside the output view, then shift
            if (window.geom.x) < (output_area.x + edge_gap + gap + bw) {
                x_correction = output_area.x + edge_gap + gap + bw - window.geom.x;
            }
            if (window.geom.x + window.geom.w)>(output_area.x+output_area.w - edge_gap - gap - bw) {
                x_correction = -((window.geom.x + window.geom.w) - (output_area.x + output_area.w) + edge_gap + gap + bw);
            }
            first_win = false;
        }
        window.geom.x += x_correction;
        column.geom.x = window.geom.x;

        // Draw borders
        let mut this_border_color = &config.window.border_color_unfocused;
        if window.proxy.id()==focused_window.proxy.id() && is_focused_output {
            this_border_color = &config.window.border_color_focused;
        }
        let bcol = parse_hex_color(this_border_color.as_str());
        window.proxy.set_borders(Edges::all(), bw, bcol.0, bcol.1, bcol.2, bcol.3);

        println!(" |---> focused column id = {} / window id = {} / window.geom = {}x{}+{}+{}", column.id, window.proxy.id(), window.geom.w, window.geom.h, window.geom.x, window.geom.y);
        window.node.set_position(window.geom.x, window.geom.y);
    }
}

fn render_unfocused_column(
    direction: Direction, 
    column: &mut Column,
    windows: &mut HashMap<ObjectId, Window>,
    x_offset: i32,
    output_area: Geometry,
    config: &Config,
) {

    let bw = config.window.border_width;
    let gap = config.layout.gap;

    let mut y_offset = output_area.y + gap + bw;

    for window_id in &column.windows_id {
        let window = windows.get_mut(&window_id).unwrap();

        if window.is_floating { 
            let this_border_color = &config.window.border_color_unfocused;
            let bcol = crate::wmcore::parse_hex_color(this_border_color.as_str());
            window.proxy.set_borders(Edges::all(), bw, bcol.0, bcol.1, bcol.2, bcol.3 );
        
            window.node.place_top();

            continue; 
        }

        window.at_scroll_edge = false;

        window.geom.y = y_offset;
        y_offset += window.geom.h + gap + 2*bw;

        window.geom.x = x_offset;
        column.geom.x = window.geom.x;

        if direction == Direction::Right {
            // if the window is completely out of range, then hide
            if window.geom.x > output_area.x + output_area.w {
                window.proxy.hide();
            } else {
                window.proxy.show();
                if window.geom.x + window.geom.w > output_area.x + output_area.w {
                    let clip_width = (output_area.x+output_area.w)-window.geom.x;
                    window.proxy.set_clip_box(-bw, -bw, clip_width+bw, window.geom.h+2*bw);
                    if output_area.x+output_area.w - window.geom.x < ((config.layout.scroll_edge_gap as f32*1.5) as i32) {
                        window.at_scroll_edge = true;
                    }
                } else {
                    window.proxy.set_clip_box(0, 0, 0, 0);
                }
            }
        } else {
            if window.geom.x+window.geom.w < output_area.x {
                window.proxy.hide();
            } else {
                window.proxy.show();
                if window.geom.x < output_area.x {
                    let clip_x = output_area.x-window.geom.x;
                    let clip_width = window.geom.x+window.geom.w - output_area.x;
                    window.proxy.set_clip_box(clip_x, -bw, clip_width+bw, window.geom.h+2*bw);
                    if window.geom.x + window.geom.w - output_area.x < ((config.layout.scroll_edge_gap as f32*1.5) as i32) {
                        window.at_scroll_edge = true;
                    }
                } else {
                    window.proxy.set_clip_box(0, 0, 0, 0);
                }
            }
        }

        // Draw borders
        let this_border_color = &config.window.border_color_unfocused;
        let bcol = crate::wmcore::parse_hex_color(this_border_color.as_str());
        window.proxy.set_borders(Edges::all(), bw, bcol.0, bcol.1, bcol.2, bcol.3 );
        
        if direction == Direction::Right {
            println!(" |---> right column id = {} / window id = {} / window at scroll edge = {} / window dimension: {}x{}+{}+{}", column.id, window.proxy.id(), window.at_scroll_edge, window.geom.w, window.geom.h, window.geom.x, window.geom.y);
        } else {
            println!(" |---> left column id = {} / window id = {} / window at scroll edge = {} / window dimension: {}x{}+{}+{}", column.id, window.proxy.id(), window.at_scroll_edge, window.geom.w, window.geom.h, window.geom.x, window.geom.y);
        }
        window.node.set_position(window.geom.x, window.geom.y);
    }
}

pub fn arrange(
    state: &mut WMState
) {
    println!(" |-> [ arrange ]");
    for (output_id, output) in state.outputs.iter() {
        println!(" |--> output = {}", output_id);

        // Hide windows not in the visible tags
        for column in state.columns.iter()
            .filter(|c| &c.output_id == output_id && c.tag & output.visible_tags == 0) {
                for window in state.windows.values().filter(|w| w.column_id==column.id) {
                    window.proxy.hide();
                }
            }

        // List of columns in the output in the visible tags
        let mut visible_columns: Vec<&mut Column> = state.columns.iter_mut()
            .filter(|c| &c.output_id == output_id && c.tag & output.visible_tags > 0)
            .collect();

        let ncols_visible = visible_columns.len();
        if ncols_visible == 0 { continue; }
        println!("ncols_visible = {ncols_visible}");

        let focused_window = match state.windows.get(&output.focused_window_id) {
            Some(window) => window.clone(),
            None => state.windows.values().find(|w| w.column_id == visible_columns[0].id).unwrap().clone(),
        };
        //println!(" |---> focused window = {}", focused_window.proxy.id());
        
        // Don't need to do anything if focused window is fullscreen mode
        if focused_window.is_fullscreen { continue; }

        let focused_column_index = match visible_columns.iter().position(|c| c.id == focused_window.column_id) {
            Some(index) => index,
            None => visible_columns.len()-1, 
        };
        println!(" |---> focused window id = {} / column index = {focused_column_index} / id = {}", focused_window.proxy.id(), focused_window.column_id);

        let bw = state.config.window.border_width;
        let gap = state.config.layout.gap;
        let area = output.usable_area;

        // Compute the x_offset first
        let mut x_offset = area.x;
        if let Some(previous_column) = visible_columns.get(focused_column_index-1) {
            x_offset = previous_column.geom.x + previous_column.geom.w + gap + 2*bw;
        }
        let focused_column = visible_columns.get_mut(focused_column_index).unwrap();
        if focused_column.center_requested {
            x_offset = (area.x+area.w-focused_column.geom.w)/2;
            focused_column.center_requested = false;
        }

        //Do the focused column first
        //println!(" focused_column width = {} / output_id = {}", focused_column.geom.w, focused_column.output_id);
        if focused_column.geom.w == 0 {
            for window_id in &focused_column.windows_id { 
                let window = state.windows.get_mut(&window_id).unwrap();

                let this_border_color = &state.config.window.border_color_focused;
                let bcol = parse_hex_color(this_border_color.as_str());
                window.proxy.set_borders(Edges::all(), bw, bcol.0, bcol.1, bcol.2, bcol.3);
                window.node.place_top();

                focused_column.geom.x = x_offset;
            }
        } else {
            render_focused_column(
                focused_column, 
                &mut state.windows, 
                &focused_window,
                x_offset,
                area,
                &state.config,
                focused_column.output_id==state.focused_output_id,
            );
        }

        // Iterate from the focused window to the end of the vector 
        let focused_column = visible_columns.get(focused_column_index).unwrap();
        if focused_column.geom.w==0 {
            x_offset = focused_column.geom.x + bw;
        } else {
            x_offset = focused_column.geom.x + focused_column.geom.w + gap + 2*bw;
        }
        for i in (focused_column_index+1)..ncols_visible {
            let column = visible_columns.get_mut(i).unwrap();
            if column.geom.w == 0 { 
                for window_id in &column.windows_id {
                    let window = state.windows.get_mut(&window_id).unwrap();

                    let this_border_color = &state.config.window.border_color_unfocused;
                    let bcol = parse_hex_color(this_border_color.as_str());
                    window.proxy.set_borders(Edges::all(), bw, bcol.0, bcol.1, bcol.2, bcol.3 );

                    window.node.place_top();
                }
                continue; 
            }

            render_unfocused_column(
                Direction::Right,
                column, 
                &mut state.windows, 
                x_offset,
                area,
                &state.config,
            );
            x_offset += column.geom.w + gap + 2*bw;
        }

        // Iterate from the focused window to the beginning of the vector backwards 
        let focused_column = visible_columns.get(focused_column_index).unwrap();
        x_offset = focused_column.geom.x;
        for i in (0..focused_column_index).rev() {
            let column = visible_columns.get_mut(i).unwrap();
            if column.geom.w == 0 { 
                for window_id in &column.windows_id {
                    let window = state.windows.get_mut(&window_id).unwrap();

                    if window.is_floating { 
                        let this_border_color = &state.config.window.border_color_unfocused;
                        let bcol = crate::wmcore::parse_hex_color(this_border_color.as_str());
                        window.proxy.set_borders(Edges::all(), bw, bcol.0, bcol.1, bcol.2, bcol.3 );

                        window.node.place_top();
                    }
                }
                continue; 
            }

            x_offset -= column.geom.w + gap + 2*bw;
            render_unfocused_column(
                Direction::Left,
                column, 
                &mut state.windows, 
                x_offset,
                area,
                &state.config,
            );
        }
    }
}
