<h1 align="center"> &#x1f3de mackenzie</h1>

A simple window manager/compositor for [River](https://codeberg.org/river/river). It is written to fit my specific needs, and a way to teach myself how to code in rust. 


## Description

  - Named after the longest river in Canada, the [Mackenzie] river. 
  - Minimal scrolling layout, heavily inspired by [niri].
  - Written in [rust]
  - Built upon [tinyrwm]
  - Features:
    - No frills (menu, titlebar, icons, animation*, pixmap themes, etc...)
    - Configration file uses [toml] format
    - Basic IPC call for use with [quickshell]
    - Easy to read code structure so others can easily understand, reference and learn from

\* may be added later

[niri]: https://niri-wm.github.io/niri/index.html
[rust]: https://rust-lang.org
[tinyrwm]: https://codeberg.org/river/tinyrwm/
[toml]: https://toml.io/en/
[quickshell]: https://quickshell.org/ 
[Mackenzie]: https://en.wikipedia.org/wiki/Mackenzie_River


### Screenshots

Will be added soon!


### Building

  - Build dependencies
    - river >= 0.4.x
    - toml
    - lixkbcommon


```bash
cargo build --release
sudo cp target/release/mackenzie /usr/local/bin/
sudo cp mackenzie.desktop /usr/local/share/wayland-sessions/
```

### Usage

Mackenzie needs to be invoked using River:

```bash
river -c mackenzie
```

or add a line to exec mackenzie in the river init file `~/.config/river/init`


For use as an ipc client:

```bash
mackenzie (--get|--watch) tags
           --get          status
           --version
           --action       focus_tag 1
                          focus left
                          quit 
                          ... etc
```

### Configuration

Default configuration file is read from `$HOME/.config/river/mackenzie.toml`.

## Version Log

Please use the [Github Issues Tracker][ghit] to report bugs and issues.


  - 0.1 (work in progress)
    - Goal: get the base code in working order


[ghit]: https://github.com/kcirick/mackenzie/issues
