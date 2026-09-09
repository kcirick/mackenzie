
use wayland_backend::client::ObjectId;
use crate::protocol::river_wm::river_window_v1::Edges;
use wayland_client::Proxy;

use std::collections::HashMap;

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
    pub id: i32,
    pub output_id: ObjectId,
    pub windows_id: Vec<ObjectId>,

    pub x_pos: i32,
    pub width: i32,
    pub prev_width: i32,

    pub tags: u16,
    pub is_maximized: bool,
    pub redistribute_requested: bool,
}

//--- Functions -----
pub fn create_new_column (
    columns: &mut Vec<Column>,
    new_column_id: i32,
    new_column_index: usize,
    window: &mut Window,
    output: &Output,
    focused_tag: u16,
) {
    let mut column = Column { 
        id: new_column_id, 
        x_pos: window.geom.x,
        width: window.geom.w,
        prev_width: window.geom.w,
        output_id: output.proxy.id().clone(),
        windows_id: Vec::new(),
        tags: focused_tag,
        is_maximized: false,
        redistribute_requested: false,
    };

    window.column_id = new_column_id;
    window.output_id = output.proxy.id().clone();

    column.windows_id.push(window.proxy.id().clone());

    if columns.len() == 0 {
        columns.push(column);
    } else { 
        columns.insert(new_column_index, column);
    }
    println!(" |--> Created a new column with id = {} at position {}", new_column_id, new_column_index);
}

fn render_focused_column(
    column: &mut Column,
    windows: &mut HashMap<ObjectId, Window>,
    focused_window: &Window,
    x_offset: i32,
    output_area: Geometry,
    config: &Config,
) {

    let bw = config.window.border_width;
    let gap = config.layout.gap + bw;
    let edge_gap = config.layout.scroll_edge_gap; 

    let mut y_offset = output_area.y + gap;
    let mut first_win = true;
    let mut x_correction = 0;

    for window_id in &column.windows_id { 
        let window = windows.get_mut(&window_id).unwrap();

        window.at_scroll_edge = false;

        window.geom.x = x_offset;
        window.geom.y = y_offset;
        y_offset += window.geom.h + gap;

        // Reset the clip box for focused column
        window.proxy.show();
        window.proxy.set_clip_box(0, 0, 0, 0);

        if first_win {
            // If the geometry is outside the output view, then shift
            if (window.geom.x) < (output_area.x + edge_gap + gap) {
                x_correction = output_area.x - window.geom.x + edge_gap + gap;
            }
            if (window.geom.x + window.geom.w)>(output_area.x+output_area.w - edge_gap - gap) {
                x_correction = -((window.geom.x + window.geom.w) - (output_area.x + output_area.w) + edge_gap + gap);
            }
            first_win = false;
        }
        window.geom.x += x_correction;
        column.x_pos = window.geom.x;

        // Draw borders
        let mut this_border_color = &config.window.border_color_unfocused;
        if window.proxy.id()==focused_window.proxy.id() {
            this_border_color = &config.window.border_color_focused;
        }
        let bcol = crate::wmcore::parse_hex_color(this_border_color.as_str());
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
    let gap = config.layout.gap + bw;

    let mut y_offset = output_area.y + gap;

    for window_id in &column.windows_id {
        let window = windows.get_mut(&window_id).unwrap();

        window.at_scroll_edge = false;

        window.geom.y = y_offset;
        y_offset += window.geom.h + gap;

        window.geom.x = x_offset;
        column.x_pos = window.geom.x;

        // if the window is completely out of range, then hide
        if direction == Direction::Right {
            if window.geom.x > output_area.x + output_area.w {
                window.proxy.hide();
            } else {
                window.proxy.show();
                if window.geom.x + window.geom.w > output_area.x + output_area.w {
                    let clip_width = (output_area.x+output_area.w)-window.geom.x;
                    window.proxy.set_clip_box(-bw, -bw, clip_width+bw, window.geom.h+2*bw);
                    if output_area.x+output_area.w - window.geom.x < config.layout.scroll_edge_gap {
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
                    if window.geom.x + window.geom.w - output_area.x < config.layout.scroll_edge_gap {
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
    for (output_id, output) in &state.outputs {
        println!(" |--> output = {}", output_id);

        // Hide windows not in the visible tags
        for column in state.columns.iter().filter(|c| &c.output_id == output_id && c.tags & output.visible_tags == 0) {
            for window in state.windows.values().filter(|w| w.column_id==column.id) {
                window.proxy.hide();
            }
        }

        let mut visible_columns: Vec<&mut Column> = state.columns.iter_mut()
            .filter(|c| &c.output_id == output_id && c.tags & output.visible_tags > 0)
            .collect();
        let ncols = visible_columns.len();
        if ncols==0 { continue; }

        let focused_window = state.windows.get(&state.focused_window_id).expect("").clone();
        //if &focused_window.output_id != output_id { continue };
        println!(" |---> focused window = {}", focused_window.proxy.id());

        let focused_column_index = match visible_columns.iter().position(|c| c.id == focused_window.column_id) {
            Some(index) => index,
            None => visible_columns.len()-1, 
        };
        println!(" |---> focused column index = {focused_column_index} / id = {}", focused_window.column_id);

        let bw = state.config.window.border_width;
        let gap = state.config.layout.gap + bw;
        let edge_gap = state.config.layout.scroll_edge_gap; 
        let area = output.usable_area;

        // Compute the x_offset first
        let mut x_offset = area.x + edge_gap + gap;
        if focused_column_index>0 {
            let previous_column = visible_columns.get(focused_column_index-1).unwrap();
            x_offset = previous_column.x_pos + previous_column.width + gap;
        }

        //Do the focused column first
        let focused_column = visible_columns.get_mut(focused_column_index).unwrap();
        render_focused_column(
            focused_column, 
            &mut state.windows, 
            &focused_window,
            x_offset,
            area,
            &state.config,
            );

        // Iterate from the focused window to the end of the vector 
        let focused_column = visible_columns.get(focused_column_index).unwrap();
        x_offset = focused_column.x_pos + focused_column.width + gap;
        for i in (focused_column_index+1)..ncols {
            let column = visible_columns.get_mut(i).unwrap();

            render_unfocused_column(
                Direction::Right,
                column, 
                &mut state.windows, 
                x_offset,
                area,
                &state.config,
            );
            x_offset += column.width + gap;
        }

        // Iterate from the focused window to the beginning of the vector backwards 
        let focused_column = visible_columns.get(focused_column_index).unwrap();
        x_offset = focused_column.x_pos - gap;
        for i in (0..focused_column_index).rev() {
            let column = visible_columns.get_mut(i).unwrap();

            x_offset -= column.width;
            render_unfocused_column(
                Direction::Left,
                column, 
                &mut state.windows, 
                x_offset,
                area,
                &state.config,
            );
            x_offset -= gap;
        }
    }
}
