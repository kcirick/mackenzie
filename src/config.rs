use std::fs;
use std::collections::HashMap;
use serde::Deserialize;

use crate::actions::Action;
use crate::protocol::river_wm::river_seat_v1::Modifiers;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub layout: LayoutConfig,
    pub window: WindowConfig, 
    pub inputs: InputsConfig,
    pub rules: Option<RulesConfig>,
    pub keybinds: Option<HashMap<String, ActionConfig>>,
    pub mousebinds: Option<HashMap<String, ActionConfig>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct LayoutConfig {
    pub n_tags: u16,
    pub gap: i32,
    pub scroll_edge_gap: i32,
    pub default_column_width: f32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct WindowConfig {
    pub move_resize_step: i32,
    pub border_width: i32,
    pub border_color_focused: String,
    pub border_color_unfocused: String,
    pub border_color_floating: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct InputsConfig {
    pub xkb_layout: String,
    pub xkb_options: String,
    pub touchpad_tap_click: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RulesConfig {
    pub windowrules: Vec<WindowRule>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WindowRule {
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub floating: Option<bool>,
    pub width: Option<f32>,
    pub maximized: Option<bool>,
    pub tag: Option<u16>,
    pub output: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ActionConfig {
    pub action: String,
    pub args: Option<Vec<String>>,
}

pub struct DefaultKeybind {
    pub modmask: Modifiers,
    pub key: String,
    pub action: Action,
}

//--- Set default values -----
impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            n_tags: 3,
            gap: 0,
            scroll_edge_gap: 0,
            default_column_width: 0.6,
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            move_resize_step: 10,
            border_width: 2,
            border_color_focused: "#FFFFFF".to_string(),
            border_color_unfocused: "#333333".to_string(),
            border_color_floating: "#FF7F50".to_string(),
        }
    }
}

impl Default for InputsConfig {
    fn default() -> Self {
        Self {
            xkb_layout: "us".to_string(),
            xkb_options: "".to_string(),
            touchpad_tap_click: false,
        }
    }
}

//--- Read in config file -----
impl Config {
    pub fn default() -> Self {
        Self {
            layout: LayoutConfig::default(),
            window: WindowConfig::default(),
            inputs: InputsConfig::default(),
            rules: None,
            keybinds: None,
            mousebinds: None,
        }
    }

    pub fn load_config() -> Self {
        let home = std::env::var("HOME").expect("The HOME environment variable was not found.");
        let config_file = "~/.config/river/mackenzie.toml".replace("~", &home.as_str());
        
        if let Ok(contents) = fs::read_to_string(&config_file) {
            let config: Config = toml::from_str(&contents)
                .expect("Failed to parse TOML configuration");

            return config;
        } else {
            eprintln!("Couldn't read config file: {:?}", config_file);
        }

        // Returns default config
        Config::default()
        
    }
}

pub fn load_default_keybinds() -> Vec<DefaultKeybind> {
    vec![
        DefaultKeybind {
            modmask: Modifiers::Mod1,
            key: "space".to_string(),
            action: Action::Spawn("foot".to_string(), Vec::new()),
        },
        DefaultKeybind {
            modmask: Modifiers::Mod1,
            key: "q".to_string(),
            action: Action::Exit,
        },
    ]
}
