use crate::protocol::river_wm::river_window_manager_v1::RiverWindowManagerV1;
use crate::protocol::river_wm::river_window_v1::Edges;
//use wayland_backend::client::ObjectId;
use wayland_client::Proxy;

//use std::collections::HashMap;

use crate::wmcore::WMState;
use crate::wmcore::Window;
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
    MoveToTag(String),
    ToggleMaximizeColumn,
    ConsumeEject(String),
    Close,
    Focus(String),
    Move(String),
    Resize(String),
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
            "move_to_tag" =>                Action::MoveToTag(main),
            "toggle_maximize_column" =>     Action::ToggleMaximizeColumn,
            "consume_or_eject" =>           Action::ConsumeEject(main),
            "quit" =>                       Action::Exit,
            "focus" =>                      Action::Focus(main),
            "move" =>                       Action::Move(main),
            "resize" =>                     Action::Resize(main),
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
                let focused_window = state.windows.get(&state.focused_window_id).unwrap();
                focused_window.proxy.close();
            }
            
            Action::ToggleMaximizeColumn => {
                let focused_window = state.windows.get(&state.focused_window_id).unwrap();
                let focused_column = state.columns.iter_mut().find(|c| c.id == focused_window.column_id).unwrap();

                println!("toggle_maximize_column");
                if focused_column.is_maximized {
                    focused_column.width = focused_column.prev_width;
                } else {
                    let focused_output = state.outputs.get(&state.focused_output_id).unwrap(); 
                    let output_area = focused_output.usable_area;
                    let gap = state.config.layout.gap + state.config.window.border_width;
                    let edge_gap = state.config.layout.scroll_edge_gap;

                    focused_column.prev_width = focused_column.width;
                    focused_column.width = output_area.w - 2*edge_gap - 2* gap;

                }
                println!("column width = {} / prev = {}", focused_column.width, focused_column.prev_width);
                focused_column.is_maximized = ! focused_column.is_maximized;

                for window in state.windows.values_mut().filter(|w| w.column_id==focused_column.id) {
                    window.geom.w = focused_column.width;
                    window.resize_requested = true;
                }
                state.needs_arrange = true;
            }

            Action::FocusTag(tag_str) => {
                let tag: i16 = tag_str.parse().expect("Not a valid number");
                let focused_output = state.outputs.get_mut(&state.focused_output_id).unwrap();

                println!("Focusing tag {tag}");
                state.focused_tag = 1<<(tag-1);
                focused_output.visible_tags = 1<<(tag-1);

                println!("needs_arrange from focus_tag action"); 
                state.needs_arrange = true;
            }
            
            Action::MoveToTag(tag_str) => {
                let tag: i16 = tag_str.parse().expect("Not a valid number");
                let focused_window = state.windows.get(&state.focused_window_id).unwrap();
                let focused_column = state.columns.iter_mut().find(|c| c.id == focused_window.column_id).unwrap();

                println!("Moving to tag {tag}");
                focused_column.tags = 1<<(tag-1);
                seat.hovered = None;

                println!("needs_arrange from move_to_tag action"); 
                state.needs_arrange = true;
            }
            
            Action::Move(direction) => {
                if direction.len()>0  {
                    println!("direction = {}", &direction);
                } else {
                    if let (Some(window_proxy), SeatOp::None) = (seat.hovered.as_ref(), &seat.op) {
                        let window = state.windows.get(&window_proxy.id())
                            .expect("Hovered window not found");
                        seat.pointer_move(window);
                    }
                }
            }
            
            Action::ConsumeEject(direction) => {
                if direction.len()==0 { return; }
                println!("consume or eject {direction}");

                let max_column_id = state.windows.iter()
                    .max_by_key(|(_, w)| w.column_id)
                    .map(|(_, w)| w.column_id).unwrap();
                let columns_length = state.columns.len().clone();

                let focused_output = state.outputs.get(&state.focused_output_id).unwrap(); 
                let focused_window = state.windows.get(&state.focused_window_id).unwrap();
                let focused_column_id = focused_window.column_id.clone();
                let (focused_column_index, focused_column) = state.columns.iter_mut().enumerate()
                    .find(|(_, c)| c.id == focused_column_id).unwrap();

                let n_wins = focused_column.windows_id.len() as i32;
                let focused_window = state.windows.get_mut(&state.focused_window_id).unwrap();
                let gap = state.config.layout.gap;
                if direction.as_str() == "left" {
                    // consume to next column
                    if n_wins == 1 && focused_column_index>0 {
                        if let Some((_, next_column)) = state.columns.iter_mut().enumerate().rev()
                            .find(|(i, c)| 
                                i<&focused_column_index && 
                                c.output_id == state.focused_output_id && 
                                c.tags & focused_output.visible_tags > 0) 
                        { 
                            //println!("next column: {ind} - {:?}", next_column);
                            focused_window.column_id=next_column.id.clone();
                            next_column.windows_id.push(focused_window.proxy.id().clone());

                            let n_wins_2 = next_column.windows_id.len() as i32;
                            let windows: Vec<&mut Window> = state.windows.values_mut().filter(|w| w.column_id==next_column.id).collect();
                            for window in windows {
                                window.geom.w = next_column.width;
                                window.geom.h = (focused_output.usable_area.h - (n_wins_2+1)*gap)/n_wins_2;
                                window.proxy.propose_dimensions(window.geom.w, window.geom.h);
                            }
                        }
                    }
                    // eject to a new column
                    else {
                        create_new_column(
                            &mut state.columns,
                            max_column_id+1,
                            focused_column_index,
                            focused_window,
                            focused_output,
                            state.focused_tag,
                        );
                        let prev_column = state.columns.iter_mut().find(|c| c.id == focused_column_id).unwrap();
                        prev_column.windows_id.retain(|wid| wid != &state.focused_window_id);
                        focused_window.geom.h = focused_output.usable_area.h - 2*gap;
                        focused_window.proxy.propose_dimensions(focused_window.geom.w, focused_window.geom.h);

                        let n_wins_2 = prev_column.windows_id.len() as i32;
                        let windows: Vec<&mut Window> = state.windows.values_mut().filter(|w| w.column_id==prev_column.id).collect();
                        for window in windows {
                            window.geom.w = prev_column.width;
                            window.geom.h = (focused_output.usable_area.h - (n_wins_2+1)*gap)/n_wins_2;
                            window.proxy.propose_dimensions(window.geom.w, window.geom.h);
                        }
                    }
                }
                else if direction.as_str() == "right" {
                    if n_wins == 1 && focused_column_index<(columns_length-1) {
                        if let Some((_, next_column)) = state.columns.iter_mut().enumerate()
                            .find(|(i, c)| 
                                i>&focused_column_index && 
                                c.output_id == state.focused_output_id && 
                                c.tags & focused_output.visible_tags > 0) 
                        { 
                            focused_window.column_id=next_column.id.clone();
                            next_column.windows_id.push(focused_window.proxy.id().clone());

                            let n_wins_2 = next_column.windows_id.len() as i32;
                            let windows: Vec<&mut Window> = state.windows.values_mut().filter(|w| w.column_id==next_column.id).collect();
                            for window in windows {
                                window.geom.w = next_column.width;
                                window.geom.h = (focused_output.usable_area.h - (n_wins_2+1)*gap)/n_wins_2;
                                window.proxy.propose_dimensions(window.geom.w, window.geom.h);
                            }
                        }
                    }
                    else {
                        create_new_column(
                            &mut state.columns,
                            max_column_id+1,
                            focused_column_index+1,
                            focused_window,
                            focused_output,
                            state.focused_tag,
                        );
                        let prev_column = state.columns.iter_mut().find(|c| c.id == focused_column_id).unwrap();
                        prev_column.windows_id.retain(|wid| wid != &state.focused_window_id);

                        focused_window.geom.h = focused_output.usable_area.h - 2*gap;
                        focused_window.proxy.propose_dimensions(focused_window.geom.w, focused_window.geom.h);

                        let n_wins_2 = prev_column.windows_id.len() as i32;
                        let windows: Vec<&mut Window> = state.windows.values_mut().filter(|w| w.column_id==prev_column.id).collect();
                        for window in windows {
                            window.geom.w = prev_column.width;
                            window.geom.h = (focused_output.usable_area.h - (n_wins_2+1)*gap)/n_wins_2;
                            window.proxy.propose_dimensions(window.geom.w, window.geom.h);
                        }

                    }
                }
                println!("needs_arrange from consume/eject action"); 
                state.needs_arrange = true;
            }
            
            Action::Focus(direction) => {
                if direction.len()==0 {return; }

                let columns_length = state.columns.len().clone();

                let focused_output = state.outputs.get_mut(&state.focused_output_id).unwrap(); 
                let focused_window = state.windows.get(&state.focused_window_id).unwrap();
                let focused_column_id = focused_window.column_id.clone();
                let (focused_column_index, focused_column) = state.columns.iter_mut().enumerate()
                    .find(|(_, c)| c.id == focused_column_id).unwrap();

                if direction.as_str() == "left" && focused_column_index>0 {
                    if let Some((_, next_column)) = state.columns.iter_mut().enumerate().rev()
                        .find(|(i, c)| 
                            i<&focused_column_index && 
                            c.output_id == state.focused_output_id && 
                            c.tags & focused_output.visible_tags > 0) 
                    { 
                        //println!("next column: {ind} - {:?}", next_column);
                        let (objid, next_window) = state.windows.iter()
                            .find(|(_,w)| w.column_id==next_column.id).unwrap();
                        state.focused_window_id = objid.clone();
                        focused_output.focused_column_id = next_window.column_id;
                        seat.proxy.focus_window(&next_window.proxy);
                    }
                }
                else if direction.as_str() == "right" && focused_column_index<(columns_length-1) {
                    if let Some((_, next_column)) = state.columns.iter_mut().enumerate()
                        .find(|(i, c)| 
                            i>&focused_column_index && 
                            c.output_id == state.focused_output_id && 
                            c.tags & focused_output.visible_tags > 0) 
                    {
                        let (objid, next_window) = state.windows.iter()
                            .find(|(_,w)| w.column_id==next_column.id).unwrap();
                        state.focused_window_id = objid.clone();
                        focused_output.focused_column_id = next_window.column_id;
                        seat.proxy.focus_window(&next_window.proxy);
                    }
                }
                else if direction.as_str() == "up" {
                    let focused_window_pos = focused_column.windows_id.iter()
                        .position(|wid| wid == &state.focused_window_id).unwrap();
                    if focused_window_pos>0 {
                        let next_window = state.windows.get(&focused_column.windows_id[focused_window_pos-1]).unwrap();
                        state.focused_window_id = next_window.proxy.id().clone();
                        seat.proxy.focus_window(&next_window.proxy);
                    }
                }
                else if direction.as_str() == "down" {
                    let focused_window_pos = focused_column.windows_id.iter()
                        .position(|wid| wid == &state.focused_window_id).unwrap();
                    if focused_window_pos<(focused_column.windows_id.len()-1) {
                        let next_window = state.windows.get(&focused_column.windows_id[focused_window_pos+1]).unwrap();
                        state.focused_window_id = next_window.proxy.id().clone();
                        seat.proxy.focus_window(&next_window.proxy);
                    }
                }
                seat.ignore_pointer_enter_event = true;
                seat.hovered = None;

                println!("needs_arrange from focus action"); 
                state.needs_arrange = true;
            }
            
            Action::Resize(direction) => {
                if direction.len()>0 {
                    println!("direction = {}", &direction);
                    let focused_window = state.windows.get(&state.focused_window_id).unwrap();
                    let focused_column = state.columns.iter_mut().find(|c| c.id == focused_window.column_id).unwrap();
                    if direction.as_str() =="left" {
                        focused_column.width -= state.config.window.move_resize_step;

                        for wid in &focused_column.windows_id {
                            let window = state.windows.get_mut(wid).unwrap();
                            window.geom.w = focused_column.width;
                            window.resize_requested = true;
                        }
                    } else if direction.as_str() == "right" {
                        focused_column.width += state.config.window.move_resize_step;

                        for wid in &focused_column.windows_id {
                            let window = state.windows.get_mut(wid).unwrap();
                            window.geom.w = focused_column.width;
                            window.resize_requested = true;
                        }
                    } else if direction.as_str() == "up" {
                        let n_wins = focused_column.windows_id.len();
                        if n_wins == 1 { return; }

                        let focused_window_position = focused_column.windows_id.iter()
                            .position(|wid| wid == &state.focused_window_id).unwrap();

                        // it's the last window in the column. i.e. at the bottom
                        if focused_window_position == n_wins-1 {
                            let prev_window_position = &focused_column.windows_id[focused_window_position-1];
                            let prev_window = state.windows.get_mut(prev_window_position).unwrap();
                            prev_window.geom.h -= state.config.window.move_resize_step;
                            prev_window.resize_requested = true;

                            let focused_window = state.windows.get_mut(&state.focused_window_id).unwrap();
                            focused_window.geom.h += state.config.window.move_resize_step;
                            focused_window.geom.x -= state.config.window.move_resize_step;
                            focused_window.resize_requested = true;
                        } else {
                            let next_window_position = &focused_column.windows_id[focused_window_position+1];
                            let next_window = state.windows.get_mut(next_window_position).unwrap();
                            next_window.geom.h += state.config.window.move_resize_step;
                            next_window.geom.x -= state.config.window.move_resize_step;
                            next_window.resize_requested = true;

                            let focused_window = state.windows.get_mut(&state.focused_window_id).unwrap();
                            focused_window.geom.h -= state.config.window.move_resize_step;
                            focused_window.resize_requested = true;
                        }
                    } else if direction.as_str() == "down" {
                        let n_wins = focused_column.windows_id.len();
                        if n_wins == 1 { return; }

                        let focused_window_position = focused_column.windows_id.iter()
                            .position(|wid| wid == &state.focused_window_id).unwrap();

                        // it's the last window in the column. i.e. at the bottom
                        if focused_window_position == n_wins-1 {
                            let prev_window_position = &focused_column.windows_id[focused_window_position-1];
                            let prev_window = state.windows.get_mut(prev_window_position).unwrap();
                            prev_window.geom.h += state.config.window.move_resize_step;
                            prev_window.resize_requested = true;

                            let focused_window = state.windows.get_mut(&state.focused_window_id).unwrap();
                            focused_window.geom.h -= state.config.window.move_resize_step;
                            focused_window.geom.x += state.config.window.move_resize_step;
                            focused_window.resize_requested = true;
                        } else {
                            let next_window_position = &focused_column.windows_id[focused_window_position+1];
                            let next_window = state.windows.get_mut(next_window_position).unwrap();
                            next_window.geom.h -= state.config.window.move_resize_step;
                            next_window.geom.x += state.config.window.move_resize_step;
                            next_window.resize_requested = true;

                            let focused_window = state.windows.get_mut(&state.focused_window_id).unwrap();
                            focused_window.geom.h += state.config.window.move_resize_step;
                            focused_window.resize_requested = true;
                        }
                    }
                    println!("needs_arrange from resize action"); 
                    state.needs_arrange = true;
                } else {
                    if let (Some(window_proxy), SeatOp::None) = (seat.hovered.as_ref(), &seat.op) {
                        let window = state.windows.get(&window_proxy.id())
                            .expect("Hovered window not found");
                        seat.pointer_resize(window, Edges::Bottom.union(Edges::Right));
                    }
                }
            }
            
            Action::Exit => wm_proxy.exit_session(),
        }
        seat.pending_action = Action::None;
    }
}
