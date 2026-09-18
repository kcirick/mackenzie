use crate::protocol::river_wm::river_window_manager_v1::RiverWindowManagerV1;
use crate::protocol::river_wm::river_window_v1::Edges;
use wayland_client::Proxy;
//use wayland_backend::client::ObjectId;

//use std::collections::HashMap;

use crate::wmcore::WMState;
//use crate::wmcore::Window;
//use crate::layout::Column;
use crate::layout::create_new_column;
//use crate::config::Config;
//use crate::output::Output;
//use crate::seat::Seat;
use crate::seat::SeatOp;

//--- Enums -----
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    Spawn(String, Vec<String>),
    FocusTag(String),
    FocusOutput(String),
    MoveToTag(String),
    MoveToOutput(String),
    ToggleTag(String),
    ToggleMaximizeColumn,
    ToggleFloat,
    SwitchColumns(String),
    Close,
    Focus(String),
    Move(String),
    MoveFloating,
    Resize(String),
    ResizeFloating,
    Exit,
}


//--- Implementations -----
impl Action {
    pub fn action_from_name(
        name: String,
        args: &Option<Vec<String>>,
    ) -> Self {
        let mut this_args  = args.clone().unwrap_or_default();
        let mut main: String = "".to_string();
        let mut rest: Vec<String> = Vec::new();
        if this_args.len()>0 {
            main = this_args[0].clone();
            rest = this_args.split_off(1);
        }
        match name.to_lowercase().as_str() {
            "spawn" =>                      Action::Spawn(main, rest),
            "focus_tag" =>                  Action::FocusTag(main),
            "focus_output" =>               Action::FocusOutput(main),
            "toggle_tag" =>                 Action::ToggleTag(main),
            "move_to_tag" =>                Action::MoveToTag(main),
            "move_to_output" =>             Action::MoveToOutput(main),
            "toggle_maximize_column" =>     Action::ToggleMaximizeColumn,
            "toggle_float" =>               Action::ToggleFloat,
            "switch_columns" =>             Action::SwitchColumns(main),
            "quit" =>                       Action::Exit,
            "focus" =>                      Action::Focus(main),
            "move" =>                       Action::Move(main),
            "move_floating" =>              Action::MoveFloating,
            "resize" =>                     Action::Resize(main),
            "resize_floating" =>            Action::ResizeFloating,
            "close" =>                      Action::Close,
            _ => {
                println!("Warning: Unknown action");
                Action::None
            }
        }
    }

    pub fn do_action(
        state: &mut WMState,
        wm_proxy: &RiverWindowManagerV1,
    ){
        let seat = state.seat.as_mut().unwrap();
        let focused_output = state.outputs.get_mut(&state.focused_output_id).unwrap(); 
        match &seat.pending_action {
            Action::None => {}

            Action::Spawn(cmd, args) => {
                if cmd.is_empty() { return; }

                // Don't pass WAYLAND_DEBUG onto the children.
                std::process::Command::new(cmd)
                    .args(args)
                    .env_remove("WAYLAND_DEBUG")
                    .spawn()
                    .map_err(|e| eprintln!("-> Spawn failed: {}", e))
                    .ok();
                }
            
            Action::Close => {
                let focused_window = state.windows.get(&focused_output.focused_window_id).unwrap();
                focused_window.proxy.close();
            }
            
            Action::ToggleMaximizeColumn => {
                let focused_column = state.columns.iter_mut()
                    .find(|c| c.windows_id.contains(&focused_output.focused_window_id)).unwrap();

                focused_column.is_maximized = ! focused_column.is_maximized;
                if focused_column.is_maximized {
                    let output_area = focused_output.usable_area;
                    let gap = state.config.layout.gap + state.config.window.border_width;

                    focused_column.prev_width = focused_column.geom.w;
                    focused_column.geom.w = output_area.w - 2*gap;
                } else {
                    focused_column.geom.w = focused_column.prev_width;
                }
                //println!("column width = {} / prev = {}", focused_column.geom.w, focused_column.prev_width);

                for window in state.windows.values_mut().filter(|w| w.column_id==focused_column.id) {
                    window.geom.w = focused_column.geom.w;
                    window.resize_requested = true;
                }
                println!("needs_arrange from toggle_maximimize action"); 
                state.needs_arrange = true;
            }

            Action::ToggleFloat => {
                let focused_window = state.windows.get_mut(&focused_output.focused_window_id).unwrap();
                let (focused_column_index, focused_column) = state.columns.iter_mut().enumerate()
                    .find(|(_, c)| c.id == focused_window.column_id).unwrap();
                let n_wins = focused_column.windows_id.len() as i32;

                focused_window.is_floating = ! focused_window.is_floating;
                if focused_window.is_floating {
                    if n_wins==1 {
                        focused_column.geom.w = 0;
                    } else {
                        // eject into a new column if there are more than one window in a column
                        focused_column.windows_id.retain(|wid| wid != &focused_window.proxy.id());
                        focused_column.redistribute_requested = true;
                        let mut new_column = create_new_column(
                            focused_output,
                            &state.columns,
                            focused_window,
                            &state.config,
                        );
                        new_column.geom.w = 0;
                        state.columns.insert(focused_column_index, new_column);
                    }
                } else {
                    focused_column.geom.w = focused_window.geom.w;
                    focused_column.redistribute_requested = true;
                }
                println!("needs_arrange from toggle_float action"); 
                state.needs_arrange = true;
            }

            Action::FocusOutput(output_str) => {
                if output_str.len()==0 { return; }

                seat.hovered = None;
                //let focused_output_id = focused_output.proxy.id();
                if let Some(output) = state.outputs.values().find(|o| &o.name==output_str) {
                    state.focused_output_id = output.proxy.id();
                    println!("needs_arrange from focus_output action");
                    state.needs_arrange= true;
                }
                else {
                    println!("No output {output_str} was found");
                }
            }

            Action::MoveToOutput(output_str) => {
                if output_str.len()==0 { return; }

                //let focused_output_id = focused_output.proxy.id();
                let focused_output_geom = focused_output.full_area;
                let focused_column = state.columns.iter_mut()
                    .find(|c| c.windows_id.contains(&focused_output.focused_window_id)).unwrap();

                if let Some(target_output) = state.outputs.values().find(|o| &o.name==output_str) {
                    focused_column.output_id = target_output.proxy.id().clone();
                    println!("focused column id = {} / output id= {}", focused_column.id, focused_column.output_id);
                    let target_output_geom = target_output.full_area;
                    let x_diff = focused_output_geom.x - target_output_geom.x;
                    let y_diff = focused_output_geom.y - target_output_geom.y;
                    focused_column.geom.x -= x_diff;
                    focused_column.geom.y -= y_diff;
                    for wid in &focused_column.windows_id {
                        let window = state.windows.get_mut(&wid).unwrap();
                        window.geom.x -= x_diff;
                        window.geom.y -= y_diff;
                        println!("focused_column.geom.x = {} / window.geom.x = {}", focused_column.geom.x, window.geom.x);
                    }
                    println!("needs_arrange from move_to_output action");
                    state.needs_arrange= true;
                }
                else {
                    println!("No output {output_str} was found");
                }
            }

            Action::FocusTag(tag_str) => {
                let tag: i16 = tag_str.parse().expect("Not a valid number");

                println!("Focusing tag {tag}");
                focused_output.focused_tag = 1<<(tag-1);
                focused_output.visible_tags = 1<<(tag-1);

                println!("needs_arrange from focus_tag action"); 
                state.needs_arrange = true;
            }
            
            Action::ToggleTag(tag_str) => {
                let tag: i16 = tag_str.parse().expect("Not a valid number");

                println!("Toggle tag {tag}");
                if (1<<tag-1) & focused_output.visible_tags > 0 { 
                    focused_output.visible_tags ^= 1<<(tag-1);
                } else {
                    focused_output.visible_tags |= 1<<(tag-1);
                }

                println!("needs_arrange from toggle_tag action");
                state.needs_arrange= true;
            }

            Action::MoveToTag(tag_str) => {
                let tag: i16 = tag_str.parse().expect("Not a valid number");
                //let focused_window = state.windows.get(&focused_output.focused_window_id).unwrap();
                let focused_column = state.columns.iter_mut()
                    .find(|c| c.windows_id.contains(&focused_output.focused_window_id)).unwrap();

                println!("Moving to tag {tag}");
                focused_column.tag = 1<<(tag-1);
                seat.hovered = None;

                println!("needs_arrange from move_to_tag action"); 
                state.needs_arrange = true;
            }
            
            Action::Focus(direction) => {
                if direction.len()==0 { return; }

                let columns_length = state.columns.len().clone();
                let (focused_column_index, focused_column) = state.columns.iter_mut().enumerate()
                    .find(|(_, c)| c.windows_id.contains(&focused_output.focused_window_id)).unwrap();

                if direction.as_str() == "left" && focused_column_index>0 {
                    if let Some((_, next_column)) = state.columns.iter_mut().enumerate().rev()
                        .find(|(i, c)| 
                            i<&focused_column_index && 
                            c.output_id == state.focused_output_id && 
                            c.tag & focused_output.visible_tags > 0 &&
                            c.geom.w > 0) 
                    { 
                        let (objid, next_window) = state.windows.iter()
                            .find(|(_,w)| w.column_id==next_column.id).unwrap();
                        focused_output.focused_window_id = objid.clone();
                        seat.proxy.focus_window(&next_window.proxy);
                    }
                }
                else if direction.as_str() == "right" && focused_column_index<(columns_length-1) {
                    if let Some((_, next_column)) = state.columns.iter_mut().enumerate()
                        .find(|(i, c)| 
                            i>&focused_column_index && 
                            c.output_id == state.focused_output_id && 
                            c.tag & focused_output.visible_tags > 0 &&
                            c.geom.w > 0) 
                    {
                        let (objid, next_window) = state.windows.iter()
                            .find(|(_,w)| w.column_id==next_column.id).unwrap();
                        focused_output.focused_window_id = objid.clone();
                        seat.proxy.focus_window(&next_window.proxy);
                    }
                }
                else if direction.as_str() == "up" {
                    let focused_window_pos = focused_column.windows_id.iter()
                        .position(|wid| wid == &focused_output.focused_window_id).unwrap();
                    if focused_window_pos>0 {
                        let next_window = state.windows.get(&focused_column.windows_id[focused_window_pos-1]).unwrap();
                        focused_output.focused_window_id = next_window.proxy.id().clone();
                        seat.proxy.focus_window(&next_window.proxy);
                    }
                }
                else if direction.as_str() == "down" {
                    let focused_window_pos = focused_column.windows_id.iter()
                        .position(|wid| wid == &focused_output.focused_window_id).unwrap();
                    if focused_window_pos<(focused_column.windows_id.len()-1) {
                        let next_window = state.windows.get(&focused_column.windows_id[focused_window_pos+1]).unwrap();
                        focused_output.focused_window_id = next_window.proxy.id().clone();
                        seat.proxy.focus_window(&next_window.proxy);
                    }
                }
                seat.hovered = None;
                seat.ignore_pointer_enter_event = true;

                println!("needs_arrange from focus action"); 
                state.needs_arrange = true;
            }
            
            Action::SwitchColumns(direction) => {
                if direction.len()==0 { return; }

                let (focused_column_index, _focused_column) = state.columns.iter_mut().enumerate()
                    .find(|(_, c)| c.windows_id.contains(&focused_output.focused_window_id)).unwrap();
                //println!("focused_column id = {} / position = {}", focused_column.id, focused_column_index);

                if direction.as_str() == "left" {
                    if let Some((next_column_index, next_column)) = state.columns.iter_mut().enumerate().rev()
                        .find(|(i, c)| 
                            i<&focused_column_index && 
                            c.output_id == state.focused_output_id && 
                            c.tag & focused_output.visible_tags > 0) 
                    { 
                        println!("column id = {} / position = {}", next_column.id, next_column_index);
                        state.columns.swap(focused_column_index, next_column_index);
                    }
                }
                else if direction.as_str() == "right" {
                    if let Some((next_column_index, next_column)) = state.columns.iter_mut().enumerate()
                        .find(|(i, c)| 
                            i>&focused_column_index && 
                            c.output_id == state.focused_output_id && 
                            c.tag & focused_output.visible_tags > 0) 
                    { 
                        println!("column id = {} / position = {}", next_column.id, next_column_index);
                        state.columns.swap(focused_column_index, next_column_index);
                    }
                }
                println!("needs_arrange from switch_columns action"); 
                state.needs_arrange = true;
            }
            
            Action::Move(direction) => {
                if direction.len()==0 { return; }

                let columns_length = state.columns.len().clone();

                let (focused_column_index, focused_column) = state.columns.iter_mut().enumerate()
                    .find(|(_, c)| c.windows_id.contains(&focused_output.focused_window_id)).unwrap();
                let focused_column_id = focused_column.id.clone();

                let n_wins = focused_column.windows_id.len() as i32;
                let focused_window = state.windows.get_mut(&focused_output.focused_window_id).unwrap();
                if direction.as_str() == "left" {
                    // consume to next column
                    if n_wins == 1 && focused_column_index>0 {
                        if let Some((_, next_column)) = state.columns.iter_mut().enumerate().rev()
                            .find(|(i, c)| 
                                i<&focused_column_index && 
                                c.output_id == state.focused_output_id && 
                                c.tag & focused_output.visible_tags > 0) 
                        { 
                            focused_window.column_id=next_column.id.clone();
                            next_column.windows_id.push(focused_window.proxy.id().clone());
                            next_column.redistribute_requested = true;
                            next_column.align_width_requested = true;
                        }
                    }
                    // eject to a new column
                    else {
                        let new_column = create_new_column(
                            focused_output,
                            &state.columns,
                            focused_window,
                            &state.config,
                        );
                        state.columns.insert(focused_column_index, new_column.clone());

                        let prev_column = state.columns.iter_mut().find(|c| c.id == focused_column_id).unwrap();
                        prev_column.windows_id.retain(|wid| wid != &focused_output.focused_window_id);
                        prev_column.redistribute_requested = true;

                        focused_window.geom.h = new_column.geom.h;
                        focused_window.proxy.propose_dimensions(focused_window.geom.w, focused_window.geom.h);
                    }
                }
                else if direction.as_str() == "right" {
                    if n_wins == 1 && focused_column_index<(columns_length-1) {
                        if let Some((_, next_column)) = state.columns.iter_mut().enumerate()
                            .find(|(i, c)| 
                                i>&focused_column_index && 
                                c.output_id == state.focused_output_id && 
                                c.tag & focused_output.visible_tags > 0) 
                        { 
                            focused_window.column_id=next_column.id.clone();
                            next_column.windows_id.push(focused_window.proxy.id().clone());
                            next_column.redistribute_requested = true;
                            next_column.align_width_requested = true;
                        }
                    }
                    else {
                        let new_column = create_new_column(
                            focused_output,
                            &state.columns,
                            focused_window,
                            &state.config,
                        );
                        state.columns.insert(focused_column_index, new_column.clone());

                        let prev_column = state.columns.iter_mut().find(|c| c.id == focused_column_id).unwrap();
                        prev_column.windows_id.retain(|wid| wid != &focused_output.focused_window_id);
                        prev_column.redistribute_requested = true;

                        focused_window.geom.h = new_column.geom.h;
                        focused_window.proxy.propose_dimensions(focused_window.geom.w, focused_window.geom.h);
                    }
                }
                else if direction.as_str() == "up" {
                    let n_wins = focused_column.windows_id.len();
                    if n_wins == 1 { return; }

                    let focused_window_position = focused_column.windows_id.iter()
                        .position(|wid| wid == &focused_output.focused_window_id).unwrap();
                    
                    if focused_window_position>0  { 
                        focused_column.windows_id.swap(focused_window_position, focused_window_position-1);
                    }
                }
                else if direction.as_str() == "down" {
                    let n_wins = focused_column.windows_id.len();
                    if n_wins == 1 { return; }

                    let focused_window_position = focused_column.windows_id.iter()
                        .position(|wid| wid == &focused_output.focused_window_id).unwrap();
                    
                    if focused_window_position < n_wins-1 {
                        focused_column.windows_id.swap(focused_window_position, focused_window_position+1);
                    }
                }
                println!("needs_arrange from move action"); 
                state.needs_arrange = true;
            }

            Action::Resize(direction) => {
                if direction.len()==0 { return; }

                //let focused_window = state.windows.get(&focused_output.focused_window_id).unwrap();
                let focused_column = state.columns.iter_mut()
                    .find(|c| c.windows_id.contains(&focused_output.focused_window_id)).unwrap();
                if direction.as_str() =="left" {
                    focused_column.geom.w -= state.config.window.move_resize_step;

                    for wid in &focused_column.windows_id {
                        let window = state.windows.get_mut(wid).unwrap();
                        window.geom.w = focused_column.geom.w;
                        window.resize_requested = true;
                    }
                } else if direction.as_str() == "right" {
                    focused_column.geom.w += state.config.window.move_resize_step;

                    for wid in &focused_column.windows_id {
                        let window = state.windows.get_mut(wid).unwrap();
                        window.geom.w = focused_column.geom.w;
                        window.resize_requested = true;
                    }
                } else if direction.as_str() == "up" {
                    let n_wins = focused_column.windows_id.len();
                    if n_wins == 1 { return; }

                    let focused_window_position = focused_column.windows_id.iter()
                        .position(|wid| wid == &focused_output.focused_window_id).unwrap();

                    // it's the last window in the column. i.e. at the bottom
                    if focused_window_position == n_wins-1 {
                        let prev_window_position = &focused_column.windows_id[focused_window_position-1];
                        let prev_window = state.windows.get_mut(prev_window_position).unwrap();
                        prev_window.geom.h -= state.config.window.move_resize_step;
                        prev_window.resize_requested = true;

                        let focused_window = state.windows.get_mut(&focused_output.focused_window_id).unwrap();
                        focused_window.geom.h += state.config.window.move_resize_step;
                        focused_window.geom.x -= state.config.window.move_resize_step;
                        focused_window.resize_requested = true;
                    } else {
                        let next_window_position = &focused_column.windows_id[focused_window_position+1];
                        let next_window = state.windows.get_mut(next_window_position).unwrap();
                        next_window.geom.h += state.config.window.move_resize_step;
                        next_window.geom.x -= state.config.window.move_resize_step;
                        next_window.resize_requested = true;

                        let focused_window = state.windows.get_mut(&focused_output.focused_window_id).unwrap();
                        focused_window.geom.h -= state.config.window.move_resize_step;
                        focused_window.resize_requested = true;
                    }
                } else if direction.as_str() == "down" {
                    let n_wins = focused_column.windows_id.len();
                    if n_wins == 1 { return; }

                    let focused_window_position = focused_column.windows_id.iter()
                        .position(|wid| wid == &focused_output.focused_window_id).unwrap();

                    // it's the last window in the column. i.e. at the bottom
                    if focused_window_position == n_wins-1 {
                        let prev_window_position = &focused_column.windows_id[focused_window_position-1];
                        let prev_window = state.windows.get_mut(prev_window_position).unwrap();
                        prev_window.geom.h += state.config.window.move_resize_step;
                        prev_window.resize_requested = true;

                        let focused_window = state.windows.get_mut(&focused_output.focused_window_id).unwrap();
                        focused_window.geom.h -= state.config.window.move_resize_step;
                        focused_window.geom.x += state.config.window.move_resize_step;
                        focused_window.resize_requested = true;
                    } else {
                        let next_window_position = &focused_column.windows_id[focused_window_position+1];
                        let next_window = state.windows.get_mut(next_window_position).unwrap();
                        next_window.geom.h -= state.config.window.move_resize_step;
                        next_window.geom.x += state.config.window.move_resize_step;
                        next_window.resize_requested = true;

                        let focused_window = state.windows.get_mut(&focused_output.focused_window_id).unwrap();
                        focused_window.geom.h += state.config.window.move_resize_step;
                        focused_window.resize_requested = true;
                    }
                }
                println!("needs_arrange from resize action"); 
                state.needs_arrange = true;
            }
            
            Action::MoveFloating => {
                if let (Some(window_proxy), SeatOp::None) = (seat.hovered.as_ref(), &seat.op) {
                    let window = state.windows.get(&window_proxy.id())
                        .expect("Hovered window not found");
                    if window.is_floating {
                        seat.pointer_move(window);
                    }
                }
            }

            Action::ResizeFloating => {
                if let (Some(window_proxy), SeatOp::None) = (seat.hovered.as_ref(), &seat.op) {
                    let window = state.windows.get(&window_proxy.id())
                        .expect("Hovered window not found");
                    if window.is_floating {
                        seat.pointer_resize(window, Edges::Bottom.union(Edges::Right));
                    }
                }
            }
            
            Action::Exit => wm_proxy.exit_session(),
        }
        seat.pending_action = Action::None;
    }
}
