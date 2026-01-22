# GPU Burden
A wayland wallpaper daemon that lets you run WGSL shader code as your wallpaper, burdening your GPU with fancy animations.

https://github.com/user-attachments/assets/c9961067-5838-4748-8660-1d55ff539457

## Installing 
Clone the repo and install via `cargo install --path .`

## Usage
Run the `gpuburden` binary like you would any other wallpaper daemon. For example with my compositor Hyprland;
```
exec-once=~/.cargo/bin/gpuburden
```

Without any configuration, gpuburden will target all monitors with a default built-in shader that looks like this;

![The default shader.](https://github.com/user-attachments/assets/355efa9c-33f5-44d0-8112-65dc60704b90)

To configure it manually, create the file `~/.config/gpuburden/gpuburden.toml`. Create the `gpuburden` folder if it doesn't exist.

The file has a single array called `monitors` that takes in an array of objects that have the name of the target monitor, as well as the shader to run.
```toml
monitors = [
    {
        name = "DP-2",
        shader = "distorted-noise.wgsl"
    },   
    {
        name = "HDMI-A-1",
        shader = "distorted-noise.wgsl"
    },   
]
```

You can get the names of all your monitors via `xrandr --listmonitors`.

![An example output of xrandr](https://github.com/user-attachments/assets/a54d044c-6441-4b42-9c0b-9e44b74d2e63)

From there, you simply need to create your wgsl shader and place it inside of that same `~/.config/gpuburden` folder.

![How the gpuburden folder should be laid out.](https://github.com/user-attachments/assets/a67cb12b-614a-49f1-9f44-4c41395c1152)

All shaders receive a `FragmentInput` struct at group 0 binding 0, that looks like this;
```wgsl
struct FragmentInput {
    screen_size: vec2<u32>,
    frame: u32,
    seed: u32
};
@group(0) @binding(0) var<uniform> fragment_input: FragmentInput;
```

`screen_size` is the x and y size of the monitor, `frame` is the current frame number and `seed` is a random number between 0 and 1,000,000.

Some example shaders, including the default shader, can be found in `examples`. Feel free to copy them and use them.

## Credits 
- The [Learn WGPU](https://sotrh.github.io/learn-wgpu/) tutorial for teaching me how WGPU works.
- The WGPU users and Wayland-rs matrix chats for helping me with a couple of issues.
- My friend Janusz for coming up with the name for this project.
