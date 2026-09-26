use std::fs;
use std::collections::HashMap;
use serde::Deserialize;


#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    pub layout: LayoutConfig,
    pub window: WindowConfig, 
    pub rules: Option<RulesConfig>,
    pub keybinds: Option<HashMap<String, ActionConfig>>,
    pub mousebinds: Option<HashMap<String, ActionConfig>>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct LayoutConfig {
    pub n_tags: u16,
    pub gap: i32,
    pub scroll_edge_gap: i32,
    pub default_column_width: f32,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct WindowConfig {
    pub move_resize_step: i32,
    pub border_width: i32,
    pub border_color_focused: String,
    pub border_color_unfocused: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BorderConfig {
    pub width: i32,
    pub color: String,
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

impl Config {
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
        Config {
            layout: LayoutConfig {
                n_tags: 4,
                gap: 0,
                scroll_edge_gap: 0,
                default_column_width: 0.5,
            },
            window: WindowConfig {
                move_resize_step: 10,
                border_width: 2,
                border_color_focused: "#FFFFFF".to_string(),
                border_color_unfocused: "#333333".to_string(),
            },
            rules: None,
            keybinds: None,
            mousebinds: None,
        }
    }
}
